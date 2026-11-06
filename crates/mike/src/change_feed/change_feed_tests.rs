use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::{ChangeFeed, Drained, pump};
use crate::store::models::{ProxyEvent, ResyncReason, RowChangedOp, RowChangedTable};

fn request_changed(session: Option<&str>) -> ProxyEvent {
    ProxyEvent::RowChanged {
        table: RowChangedTable::Requests,
        id: "1".to_owned(),
        session_id: session.map(str::to_owned),
        op: RowChangedOp::Insert,
    }
}

#[test]
fn a_change_raises_the_generation_of_its_table_and_session_only() {
    let feed = ChangeFeed::new();
    let before = feed.generation();
    feed.observe(&request_changed(Some("s1")));
    assert!(feed.table_generation(RowChangedTable::Requests) > before);
    assert_eq!(feed.table_generation(RowChangedTable::Sessions), 0);
    assert!(feed.session_generation(RowChangedTable::Requests, "s1") > before);
    assert_eq!(feed.session_generation(RowChangedTable::Requests, "s2"), 0);
    assert!(feed.changed_since(&[RowChangedTable::Requests], before));
    assert!(!feed.changed_since(&[RowChangedTable::Sessions], before));
}

#[test]
fn a_new_subscriber_starts_with_one_full_pass_then_sees_only_changed_sessions() {
    let feed = ChangeFeed::new();
    let dirty = feed.subscribe_sessions(&[RowChangedTable::Requests]);
    assert_eq!(
        dirty.drain(),
        Drained {
            sessions: Vec::new(),
            everything: true
        }
    );
    feed.observe(&request_changed(Some("b")));
    feed.observe(&request_changed(Some("a")));
    feed.observe(&request_changed(Some("a")));
    feed.observe(&request_changed(None));
    feed.record(RowChangedTable::Sessions, Some("c"));
    assert_eq!(
        dirty.drain(),
        Drained {
            sessions: vec!["a".to_owned(), "b".to_owned()],
            everything: false
        }
    );
    assert_eq!(dirty.drain(), Drained::default());
}

#[test]
fn every_subscriber_drains_its_own_set() {
    let feed = ChangeFeed::new();
    let first = feed.subscribe_sessions(&[RowChangedTable::Requests]);
    let second = feed.subscribe_sessions(&[RowChangedTable::Requests]);
    first.drain();
    second.drain();
    feed.observe(&request_changed(Some("a")));
    assert_eq!(first.drain().sessions, vec!["a".to_owned()]);
    assert_eq!(second.drain().sessions, vec!["a".to_owned()]);
}

#[test]
fn a_resync_bumps_every_table_and_session_and_asks_for_one_full_pass() {
    let feed = ChangeFeed::new();
    let dirty = feed.subscribe_sessions(&[RowChangedTable::Requests]);
    dirty.drain();
    let before = feed.generation();
    feed.observe(&ProxyEvent::ResyncRequired {
        reason: ResyncReason::Recovery,
    });
    assert!(feed.table_generation(RowChangedTable::LlmCalls) > before);
    assert!(feed.session_generation(RowChangedTable::Requests, "never-seen") > before);
    assert!(dirty.drain().everything);
    assert!(!dirty.drain().everything);
}

#[tokio::test]
async fn waiting_returns_on_a_change_to_a_watched_table() -> anyhow::Result<()> {
    let feed = ChangeFeed::new();
    let since = feed.generation();
    let writer = feed.clone();
    let waiter = tokio::spawn(async move {
        writer
            .wait_for_change(&[RowChangedTable::Requests], since, Duration::from_secs(60))
            .await
    });
    feed.record(RowChangedTable::Sessions, None);
    feed.record(RowChangedTable::Requests, Some("s1"));
    let seen = tokio::time::timeout(Duration::from_secs(10), waiter).await??;
    assert!(seen > since);
    assert!(feed.changed_since(&[RowChangedTable::Requests], since));
    Ok(())
}

#[tokio::test]
async fn waiting_without_a_change_to_a_watched_table_returns_at_the_timeout() {
    let feed = ChangeFeed::new();
    feed.record(RowChangedTable::Sessions, None);
    let since = feed.generation();
    let seen = feed
        .wait_for_change(&[RowChangedTable::Requests], since, Duration::ZERO)
        .await;
    assert_eq!(seen, since);
    assert!(!feed.changed_since(&[RowChangedTable::Requests], since));
}

#[tokio::test]
async fn a_lagged_receiver_is_treated_as_a_resync() -> anyhow::Result<()> {
    let feed = ChangeFeed::new();
    let dirty = feed.subscribe_sessions(&[RowChangedTable::Requests]);
    dirty.drain();
    let (tx, rx) = broadcast::channel(2);
    for _ in 0..5 {
        tx.send(request_changed(Some("a")))?;
    }
    drop(tx);
    pump(feed.clone(), rx, CancellationToken::new()).await;
    let drained = dirty.drain();
    assert!(drained.everything, "the lag asks for a full pass");
    assert_eq!(drained.sessions, vec!["a".to_owned()]);
    Ok(())
}
