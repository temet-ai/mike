use std::time::Duration;

use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use super::relay::{StreamEnd, relay};
use super::streams::PendingFrame;

fn frame(bytes: &'static [u8], ttl: Duration) -> (PendingFrame, oneshot::Receiver<bool>) {
    let (result, rx) = oneshot::channel();
    (
        PendingFrame {
            bytes: Bytes::from_static(bytes),
            deadline: tokio::time::Instant::now() + ttl,
            result,
        },
        rx,
    )
}

#[tokio::test]
async fn partial_upstream_frame_is_completed_before_injection() -> anyhow::Result<()> {
    let (up_tx, up_rx) = mpsc::channel(4);
    let (out_tx, mut out_rx) = mpsc::channel(8);
    let (inject_tx, inject_rx) = mpsc::channel(4);
    let shutdown = CancellationToken::new();
    let task = tokio::spawn(relay(
        ReceiverStream::new(up_rx),
        out_tx,
        inject_rx,
        shutdown.clone(),
    ));
    up_tx
        .send(Ok(Bytes::from_static(b"event: upstream\r\ndata: {\"ok\":")))
        .await?;
    let first = out_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("closed"))??;
    let (injection, receipt) = frame(b"event: client_event\ndata: {}\n\n", Duration::from_secs(2));
    inject_tx.send(injection).await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(30), out_rx.recv())
            .await
            .is_err()
    );
    up_tx.send(Ok(Bytes::from_static(b"true}\r\n\r"))).await?;
    let second = out_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("closed"))??;
    assert!(
        tokio::time::timeout(Duration::from_millis(30), out_rx.recv())
            .await
            .is_err()
    );
    up_tx.send(Ok(Bytes::from_static(b"\n: next\n\n"))).await?;
    assert_eq!(
        out_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))??,
        "\n"
    );
    assert_eq!(
        out_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))??,
        "event: client_event\ndata: {}\n\n"
    );
    assert!(receipt.await?);
    assert_eq!(
        out_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))??,
        ": next\n\n"
    );
    assert_eq!(
        [first.as_ref(), second.as_ref(), b"\n"].concat(),
        b"event: upstream\r\ndata: {\"ok\":true}\r\n\r\n"
    );
    shutdown.cancel();
    assert!(matches!(task.await?, StreamEnd::Shutdown));
    Ok(())
}

#[tokio::test]
async fn incomplete_frame_times_out_without_inserting_bytes() -> anyhow::Result<()> {
    let (up_tx, up_rx) = mpsc::channel(4);
    let (out_tx, mut out_rx) = mpsc::channel(8);
    let (inject_tx, inject_rx) = mpsc::channel(4);
    let task = tokio::spawn(relay(
        ReceiverStream::new(up_rx),
        out_tx,
        inject_rx,
        CancellationToken::new(),
    ));
    up_tx.send(Ok(Bytes::from_static(b"data: partial"))).await?;
    assert!(out_rx.recv().await.is_some());
    let (injection, receipt) = frame(b"must not appear", Duration::from_millis(20));
    inject_tx.send(injection).await?;
    assert!(!tokio::time::timeout(Duration::from_secs(1), receipt).await??);
    drop(up_tx);
    assert!(matches!(task.await?, StreamEnd::Eof));
    assert!(out_rx.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn client_disconnect_closes_a_silent_upstream() -> anyhow::Result<()> {
    let (_up_tx, up_rx) = mpsc::channel(4);
    let (out_tx, out_rx) = mpsc::channel(8);
    let (_inject_tx, inject_rx) = mpsc::channel(4);
    let task = tokio::spawn(relay(
        ReceiverStream::new(up_rx),
        out_tx,
        inject_rx,
        CancellationToken::new(),
    ));
    drop(out_rx);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), task).await??,
        StreamEnd::ClientDisconnect
    ));
    Ok(())
}

#[tokio::test]
async fn upstream_backpressure_still_expires_wake_attempts() -> anyhow::Result<()> {
    let (up_tx, up_rx) = mpsc::channel(4);
    let (out_tx, mut out_rx) = mpsc::channel(1);
    let (inject_tx, inject_rx) = mpsc::channel(4);
    out_tx
        .send(Ok(Bytes::from_static(b"previous upstream bytes")))
        .await?;
    let task = tokio::spawn(relay(
        ReceiverStream::new(up_rx),
        out_tx,
        inject_rx,
        CancellationToken::new(),
    ));
    up_tx
        .send(Ok(Bytes::from_static(b"data: still intact\n\n")))
        .await?;
    let (injection, receipt) = frame(b"must not appear", Duration::from_millis(20));
    inject_tx.send(injection).await?;
    assert!(!tokio::time::timeout(Duration::from_secs(1), receipt).await??);
    assert_eq!(
        out_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))??,
        "previous upstream bytes"
    );
    assert_eq!(
        out_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("closed"))??,
        "data: still intact\n\n"
    );
    drop(out_rx);
    assert!(matches!(task.await?, StreamEnd::ClientDisconnect));
    Ok(())
}
