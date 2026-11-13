use super::tee_sse_stream;
use bytes::Bytes;
use futures::StreamExt;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::task::TaskTracker;

const PARSER_SLOTS: usize = 64;

async fn parked_parser_holds(chunk_bytes: usize) -> anyhow::Result<(usize, usize, bool)> {
    let (read_tx, mut read_rx) = mpsc::unbounded_channel();
    let stream = futures::stream::iter(0..PARSER_SLOTS + 8).map(move |index| {
        let _ = read_tx.send(index);
        Ok::<_, io::Error>(Bytes::from(vec![b'x'; chunk_bytes]))
    });
    let tracker = TaskTracker::new();
    let (body, mut parser_rx) = tee_sse_stream(stream, &tracker);
    let mut client = body.into_data_stream();
    let mut first_forwarded = None;
    for _ in 0..PARSER_SLOTS {
        let chunk = tokio::time::timeout(Duration::from_secs(1), client.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("client ended early"))??;
        first_forwarded.get_or_insert(chunk);
    }
    for _ in 0..=PARSER_SLOTS {
        read_rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("upstream reads stopped early"))?;
    }
    let queued = parser_rx.len();
    let mut held = 0;
    let mut shares_storage = false;
    while let Ok(chunk) = parser_rx.try_recv() {
        if held == 0
            && let Some(sent) = &first_forwarded
        {
            shares_storage = sent.as_ptr() == chunk.as_ptr();
        }
        held += chunk.len();
    }
    drop(client);
    tracker.close();
    tokio::time::timeout(Duration::from_secs(1), tracker.wait()).await?;
    Ok((queued, held, shares_storage))
}

#[tokio::test]
async fn a_parked_parser_bounds_slots_not_bytes_and_shares_the_clients_storage()
-> anyhow::Result<()> {
    let mut seen = Vec::new();
    for chunk_bytes in [1, 1_024, 65_536] {
        let (queued, held, shares) = parked_parser_holds(chunk_bytes).await?;
        assert_eq!(queued, PARSER_SLOTS, "chunk {chunk_bytes}");
        assert_eq!(held, PARSER_SLOTS * chunk_bytes, "chunk {chunk_bytes}");
        assert!(
            shares,
            "the parser's chunk is the client's allocation, not a copy"
        );
        seen.push(held);
    }
    assert_eq!(seen, vec![64, 65_536, 4_194_304]);
    Ok(())
}
