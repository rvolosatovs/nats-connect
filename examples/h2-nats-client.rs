use anyhow::Context;
use bytes::Bytes;
use clap::Parser;
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// NATS address to use
    #[arg(short, long, default_value = "nats://127.0.0.1:4222")]
    nats: String,

    /// Handshake data
    #[arg(short, long, default_value = "")]
    data: Bytes,

    /// Topic to connect to
    topic: String,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let Args { nats, data, topic } = Args::parse();

    let nats = async_nats::connect(nats)
        .await
        .context("failed to connect to NATS")?;
    let (conn, payload) = nats_connect::connect(nats, topic, data)
        .await
        .context("failed to connect to peer")?;
    info!(?payload, "connected to peer");
    let (h2, connection) = h2::client::handshake(conn).await?;
    tokio::spawn(async move {
        connection.await.expect("failed to connect");
    });

    let mut h2 = h2.ready().await?;
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri("https://www.example.com/")
        .body(())
        .context("failed to construct HTTP request")?;

    let (response, _) = h2
        .send_request(request, true)
        .context("failed to send request")?;
    let (head, mut body) = response.await?.into_parts();
    info!(?head, "received response");

    let mut flow_control = body.flow_control().clone();
    while let Some(chunk) = body.data().await {
        let chunk = chunk?;
        info!(?chunk, "RX");
        let _ = flow_control.release_capacity(chunk.len());
    }
    Ok(())
}
