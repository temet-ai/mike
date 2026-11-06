use super::tee_sse_stream;
use bytes::Bytes;
use futures::StreamExt;
use http_body_util::BodyExt;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::task::TaskTracker;

#[tokio::test]
async fn tee_stalled_upstream_client_drop_completes_tracker() -> anyhow::Result<()> {
    let (upstream_tx, upstream_rx) = mpsc::channel::<Result<Bytes, io::Error>>(1);
    let prefix =
        Bytes::from_static(b"event: content_block_delta\ndata: {\"text\":\"partial\"}\n\n");
    upstream_tx.send(Ok(prefix.clone())).await?;
    let stream = tokio_stream::wrappers::ReceiverStream::new(upstream_rx);
    let tracker = TaskTracker::new();
    let (body, mut parser_rx) = tee_sse_stream(stream, &tracker);
    let mut client = body.into_data_stream();

    let forwarded = tokio::time::timeout(Duration::from_secs(1), client.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("client ended before the prefix"))??;
    assert_eq!(forwarded, prefix);
    assert_eq!(parser_rx.recv().await, Some(prefix));
    assert!(!upstream_tx.is_closed());
    assert_eq!(tracker.len(), 1);

    drop(client);
    tracker.close();
    tokio::time::timeout(Duration::from_secs(1), tracker.wait()).await?;

    assert!(upstream_tx.is_closed());
    assert_eq!(parser_rx.recv().await, None);
    Ok(())
}

#[tokio::test]
async fn tee_parser_backpressure_client_drop_completes_tracker() -> anyhow::Result<()> {
    let (read_tx, mut read_rx) = mpsc::unbounded_channel();
    let stream = futures::stream::iter(0_u8..66).map(move |index| {
        let _ = read_tx.send(index);
        Ok::<_, io::Error>(Bytes::from(vec![index]))
    });
    let tracker = TaskTracker::new();
    let (body, mut parser_rx) = tee_sse_stream(stream, &tracker);
    let mut client = body.into_data_stream();

    for index in 0_u8..64 {
        let forwarded = tokio::time::timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("client ended before parser backpressure"))??;
        assert_eq!(forwarded, Bytes::from(vec![index]));
    }
    for index in 0_u8..65 {
        assert_eq!(read_rx.recv().await, Some(index));
    }
    assert_eq!(parser_rx.len(), 64);
    assert_eq!(tracker.len(), 1);

    drop(client);
    tracker.close();
    tokio::time::timeout(Duration::from_secs(1), tracker.wait()).await?;

    for index in 0_u8..64 {
        assert_eq!(parser_rx.recv().await, Some(Bytes::from(vec![index])));
    }
    assert_eq!(parser_rx.recv().await, None);
    assert_eq!(read_rx.recv().await, None);
    Ok(())
}

#[tokio::test]
async fn tee_upstream_error_preserves_prefix_and_returns_body_error() -> anyhow::Result<()> {
    let prefix = Bytes::from_static(b"data: partial\n\n");
    let stream = futures::stream::iter([
        Ok(prefix.clone()),
        Err(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "upstream reset",
        )),
    ]);
    let tracker = TaskTracker::new();
    let (body, mut parser_rx) = tee_sse_stream(stream, &tracker);
    let mut client = body.into_data_stream();

    let forwarded = client
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("client ended before the prefix"))??;
    assert_eq!(forwarded, prefix);
    let error = client
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("upstream error was not forwarded"))?;
    assert!(error.is_err());
    assert!(client.next().await.is_none());
    assert_eq!(parser_rx.recv().await, Some(prefix));
    assert_eq!(parser_rx.recv().await, None);
    tracker.close();
    tokio::time::timeout(Duration::from_secs(1), tracker.wait()).await?;
    Ok(())
}

#[tokio::test]
async fn tee_parser_drop_keeps_client_forwarding() -> anyhow::Result<()> {
    let (upstream_tx, upstream_rx) = mpsc::channel::<Result<Bytes, io::Error>>(1);
    let stream = tokio_stream::wrappers::ReceiverStream::new(upstream_rx);
    let tracker = TaskTracker::new();
    let (body, parser_rx) = tee_sse_stream(stream, &tracker);
    let payload = Bytes::from_static(b"data: intact\n\n");
    drop(parser_rx);
    upstream_tx.send(Ok(payload.clone())).await?;
    drop(upstream_tx);

    let forwarded = body.collect().await?.to_bytes();
    assert_eq!(forwarded, payload);
    tracker.close();
    tokio::time::timeout(Duration::from_secs(1), tracker.wait()).await?;
    Ok(())
}
