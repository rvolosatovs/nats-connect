use anyhow::Context;
use bytes::Bytes;
use clap::Parser;
use http::{Response, StatusCode};
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// NATS address to use
    #[arg(short, long, default_value = "nats://127.0.0.1:4222")]
    nats: String,

    /// Topic to listen on
    topic: String,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let Args { nats, topic } = Args::parse();

    let nats = async_nats::connect(nats)
        .await
        .context("failed to connect to NATS")?;
    let mut sub = nats
        .subscribe(topic)
        .await
        .context("failed to subscribe on topic")?;
    loop {
        let conn = nats_connect::accept(nats.clone(), &mut sub, |payload| {
            info!(?payload, "client connection received");
            Ok(Bytes::new())
        })
        .await
        .context("failed to accept connection from peer")?;

        tokio::spawn(async {
            let mut h2 = h2::server::handshake(conn)
                .await
                .context("failed to perform HTTP/2 handshake")?;
            while let Some(request) = h2.accept().await {
                let (request, mut respond) = request.context("failed to get request")?;
                info!(?request, "received request");

                let response = Response::builder()
                    .status(StatusCode::OK)
                    .body(())
                    .context("failed to construct response")?;
                respond
                    .send_response(response, true)
                    .context("failed to send response")?;
            }
            anyhow::Ok(())
        });
    }
}
