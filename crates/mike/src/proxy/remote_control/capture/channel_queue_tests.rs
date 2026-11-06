use std::sync::Arc;

use tokio::sync::{Mutex, Notify};

use super::{ChannelQueues, MAX_BATCH};

#[tokio::test]
async fn push_while_a_batch_is_being_written_queues_behind_one_writer() -> anyhow::Result<()> {
    const CALLS: usize = 9;
    let queues: ChannelQueues<usize> = ChannelQueues::default();
    let batches = Arc::new(Mutex::new(Vec::<Vec<usize>>::new()));
    let release = Arc::new(Notify::new());
    assert!(
        queues.push("cse_burst", 0),
        "the first call starts the writer"
    );

    let writer = {
        let batches = Arc::clone(&batches);
        let release = Arc::clone(&release);
        tokio::spawn(queues.clone().drain("cse_burst".to_owned(), move |batch| {
            let batches = Arc::clone(&batches);
            let release = Arc::clone(&release);
            async move {
                let first = batch.first() == Some(&0);
                batches.lock().await.push(batch);
                if first {
                    release.notified().await;
                }
            }
        }))
    };

    let pushers: Vec<_> = (1..CALLS)
        .map(|n| {
            let queues = queues.clone();
            tokio::spawn(async move { queues.push("cse_burst", n) })
        })
        .collect();
    let mut started_a_writer = Vec::new();
    for pusher in pushers {
        started_a_writer.push(pusher.await?);
    }
    assert!(
        started_a_writer.iter().all(|started| !started),
        "no later call may start a second writer for the same channel"
    );
    release.notify_one();
    writer.await?;

    let batches = batches.lock().await.clone();
    assert!(
        batches.len() <= 2,
        "the calls queued behind the first write drain in one batch: {batches:?}"
    );
    let mut recorded: Vec<usize> = batches.concat();
    assert_eq!(recorded.len(), CALLS);
    recorded.sort_unstable();
    assert_eq!(recorded, (0..CALLS).collect::<Vec<_>>());
    assert!(
        queues.push("cse_burst", CALLS),
        "an idle channel starts a new writer"
    );
    Ok(())
}

#[tokio::test]
async fn push_on_another_channel_starts_its_own_writer() -> anyhow::Result<()> {
    let queues: ChannelQueues<usize> = ChannelQueues::default();
    assert!(queues.push("cse_a", 1));
    assert!(queues.push("cse_b", 2));
    assert!(!queues.push("cse_a", 3));
    Ok(())
}

#[tokio::test]
async fn drain_keeps_call_order_and_caps_each_batch() -> anyhow::Result<()> {
    let queues: ChannelQueues<usize> = ChannelQueues::default();
    let total = MAX_BATCH + 5;
    for n in 0..total {
        queues.push("cse_order", n);
    }
    let batches = Arc::new(Mutex::new(Vec::<Vec<usize>>::new()));
    let sink = Arc::clone(&batches);
    queues
        .clone()
        .drain("cse_order".to_owned(), move |batch| {
            let sink = Arc::clone(&sink);
            async move { sink.lock().await.push(batch) }
        })
        .await;
    let batches = batches.lock().await.clone();
    assert_eq!(
        batches.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![MAX_BATCH, 5]
    );
    assert_eq!(batches.concat(), (0..total).collect::<Vec<_>>());
    Ok(())
}
