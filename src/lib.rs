use core::pin::Pin;
use core::task::Poll;

use anyhow::Context as _;
use async_nats::subject::ToSubject as _;
use bytes::{BufMut, Bytes};
use futures::{pin_mut, Future, Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct Connection {
    nats: async_nats::Client,
    tx: async_nats::Subject,
    rx: async_nats::Subscriber,
    rx_buffer: Option<Bytes>,
}

impl Connection {
    pub fn new(
        nats: async_nats::Client,
        tx: async_nats::Subject,
        rx: async_nats::Subscriber,
    ) -> Self {
        Self {
            nats,
            tx,
            rx,
            rx_buffer: None,
        }
    }
}

fn process_message(
    msg: async_nats::Message,
) -> std::io::Result<(Bytes, Option<async_nats::Subject>)> {
    match msg {
        async_nats::Message {
            reply,
            payload,
            status: None | Some(async_nats::StatusCode::OK),
            ..
        } => Ok((payload, reply)),
        async_nats::Message {
            status: Some(async_nats::StatusCode::NO_RESPONDERS),
            ..
        } => Err(std::io::ErrorKind::NotConnected.into()),
        async_nats::Message {
            status: Some(async_nats::StatusCode::TIMEOUT),
            ..
        } => Err(std::io::ErrorKind::TimedOut.into()),
        async_nats::Message {
            status: Some(async_nats::StatusCode::REQUEST_TERMINATED),
            ..
        } => Err(std::io::ErrorKind::UnexpectedEof.into()),
        async_nats::Message {
            status: Some(code),
            description,
            ..
        } => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            if let Some(description) = description {
                format!("received a response with code `{code}` ({description})")
            } else {
                format!("received a response with code `{code}`")
            },
        )),
    }
}

// TODO: Use proper error types
pub async fn connect(
    nats: async_nats::Client,
    subject: impl async_nats::subject::ToSubject,
    payload: Bytes,
) -> anyhow::Result<(Connection, Bytes)> {
    let reply = nats.new_inbox().to_subject();
    let mut rx = nats
        .subscribe(reply.clone())
        .await
        .context("failed to subscribe to inbox")?;
    nats.publish_with_reply(subject, reply, payload)
        .await
        .context("failed to connect to peer")?;
    let msg = rx
        .next()
        .await
        .context("failed to receive outbound subject from peer")?;
    let (payload, tx) = process_message(msg)?;
    let tx = tx.context("peer did not specify reply subject")?;
    Ok((Connection::new(nats, tx, rx), payload))
}

// TODO: Use proper error types
pub async fn accept(
    nats: async_nats::Client,
    sub: &mut async_nats::Subscriber,
    handle: impl FnOnce(Bytes) -> std::io::Result<Bytes>,
) -> anyhow::Result<Connection> {
    let msg = sub.next().await.context("failed to accept connection")?;
    let (payload, tx) = process_message(msg)?;
    let tx = tx.context("peer did not specify reply subject")?;
    let payload = handle(payload).context("failed to process handshake data")?;
    let reply = nats.new_inbox().to_subject();
    let rx = nats
        .subscribe(reply.clone())
        .await
        .context("failed to subscribe to inbox")?;
    nats.publish_with_reply(tx.clone(), reply, payload)
        .await
        .context("failed to connect to peer")?;
    Ok(Connection::new(nats, tx, rx))
}

impl AsyncWrite for Connection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let async_nats::ServerInfo { max_payload, .. } = self.nats.server_info();
        let n = buf.len().min(max_payload);
        let (buf, _) = buf.split_at(n);
        let fut = self
            .nats
            .publish(self.tx.clone(), Bytes::copy_from_slice(buf));
        pin_mut!(fut);
        match fut.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(err)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                err.to_string(),
            ))),
            Poll::Ready(Ok(())) => Poll::Ready(Ok(n)),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for Connection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut payload = if let Some(buffer) = self.rx_buffer.take() {
            buffer
        } else {
            match Pin::new(&mut self.rx).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Ready(Some(msg)) => {
                    let (payload, _) = process_message(msg)?;
                    payload
                }
            }
        };
        let cap = buf.capacity();
        if payload.len() > cap {
            self.rx_buffer = Some(payload.split_off(cap));
        }
        buf.put(payload);
        Poll::Ready(Ok(()))
    }
}
