use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

pub struct MockSseEvent {
    pub event_type: String,
    pub data: String,
    pub delay: Duration,
}

impl MockSseEvent {
    pub fn new(event_type: &str, data: &str) -> Self {
        Self {
            event_type: event_type.to_string(),
            data: data.to_string(),
            delay: Duration::ZERO,
        }
    }
}

pub async fn raw_sse_mock(
    events: Vec<MockSseEvent>,
    drop_after: Option<usize>,
) -> anyhow::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 8192];
            let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;

            let headers =
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
            if sock.write_all(headers).await.is_err() {
                return;
            }

            for (i, event) in events.iter().enumerate() {
                if let Some(drop_at) = drop_after
                    && i >= drop_at
                {
                    drop(sock);
                    return;
                }

                if event.delay > Duration::ZERO {
                    tokio::time::sleep(event.delay).await;
                }

                let line = format!("event: {}\ndata: {}\n\n", event.event_type, event.data);
                if sock.write_all(line.as_bytes()).await.is_err() {
                    return;
                }
            }
            let _ = sock.shutdown().await;
        }
    });

    Ok(addr)
}
