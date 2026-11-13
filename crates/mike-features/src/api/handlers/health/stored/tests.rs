use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap_or_default()
}

#[test]
fn age_counts_whole_seconds_since_refresh() {
    let stored = Stored {
        value: 1,
        refreshed_at: at(0),
    };
    assert_eq!(stored.age_secs(at(45)), 45);
    assert_eq!(stored.age_secs(at(-5)), 0);
}

#[test]
fn renew_takes_the_fresh_value_and_stamps_it() {
    let previous = Stored {
        value: 1,
        refreshed_at: at(0),
    };
    let renewed = renew(Some(&previous), Some(2), at(60));
    assert_eq!(
        renewed,
        Some(Stored {
            value: 2,
            refreshed_at: at(60)
        })
    );
}

#[test]
fn renew_keeps_the_old_value_and_old_stamp_when_the_read_failed() {
    let previous = Stored {
        value: 1,
        refreshed_at: at(0),
    };
    let renewed = renew(Some(&previous), None, at(60));
    assert_eq!(renewed, Some(previous));
    assert_eq!(renew::<i64>(None, None, at(60)), None);
}

#[test]
fn renewed_sections_advance_only_the_sections_that_were_read() {
    let first = StoredSections::default().renewed(
        Readings {
            knowledge: Some(serde_json::json!({"ready": 1})),
            turns: Some(TurnCounts {
                closed_unembedded: Some(2),
                open_unembedded: Some(1),
            }),
            capture_activity: None,
        },
        at(0),
    );
    let second = first.renewed(
        Readings {
            knowledge: Some(serde_json::json!({"ready": 2})),
            ..Readings::default()
        },
        at(60),
    );
    assert_eq!(
        second.knowledge.map(|section| section.refreshed_at),
        Some(at(60))
    );
    assert_eq!(second.turns, first.turns);
}

#[tokio::test]
async fn refresh_runs_once_per_tick_and_stops_when_ticks_end() {
    let (send, receive) = tokio::sync::mpsc::channel::<()>(4);
    let runs = std::sync::Arc::new(AtomicUsize::new(0));
    let counter = runs.clone();
    let task = tokio::spawn(refresh_on_ticks(
        tokio_util::sync::CancellationToken::new(),
        tokio_stream::wrappers::ReceiverStream::new(receive),
        move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        },
    ));
    assert!(send.send(()).await.is_ok());
    assert!(send.send(()).await.is_ok());
    assert!(send.send(()).await.is_ok());
    drop(send);
    assert!(task.await.is_ok());
    assert_eq!(runs.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn refresh_stops_on_shutdown_without_another_run() {
    let shutdown = tokio_util::sync::CancellationToken::new();
    shutdown.cancel();
    let runs = std::sync::Arc::new(AtomicUsize::new(0));
    let counter = runs.clone();
    refresh_on_ticks(shutdown, tokio_stream::pending::<()>(), move || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    })
    .await;
    assert_eq!(runs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_hung_read_is_cut_short_by_its_deadline() {
    let hung = std::future::pending::<Result<i64, String>>();
    let read = read_bounded(hung, std::future::ready(()), "test").await;
    assert_eq!(read, None);
}

#[tokio::test]
async fn a_read_that_finishes_before_its_deadline_keeps_its_value() {
    let done = std::future::ready(Ok::<_, String>(5));
    let read = read_bounded(done, std::future::pending(), "test").await;
    assert_eq!(read, Some(5));
}
