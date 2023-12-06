use anyhow::Context;
use bytes::Bytes;
use clap::Parser;
use tokio::io;
use tokio::try_join;
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
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let Args { nats, data, topic } = Args::parse();

    let nats = async_nats::connect(nats)
        .await
        .context("failed to connect to NATS")?;
    let (conn, payload) = nats_connect::connect(nats, topic, data)
        .await
        .context("failed to connect to peer")?;
    info!(?payload, "connected to peer");
    let (mut r, mut w) = io::split(conn);
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();
    try_join!(io::copy(&mut stdin, &mut w), io::copy(&mut r, &mut stdout))
        .context("connection failed")?;
    Ok(())
}
