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

    /// Topic to listen on
    topic: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

        info!("accepted peer connection");
        let (mut r, mut w) = io::split(conn);
        let mut stdin = io::stdin();
        let mut stdout = io::stdout();
        try_join!(io::copy(&mut stdin, &mut w), io::copy(&mut r, &mut stdout))
            .context("connection failed")?;
    }
}
