use std::time::Duration;

use super::wait_for_mail;
use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::RowChangedTable;

fn fed_feed() -> ChangeFeed {
    let feed = ChangeFeed::default();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    crate::change_feed::spawn_change_feed(
        &feed,
        None,
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

async fn finishes(feed: &ChangeFeed, busy: bool, since: Generation) -> bool {
    tokio::time::timeout(Duration::from_millis(300), wait_for_mail(feed, busy, since))
        .await
        .is_ok()
}

async fn ticks_on_the_interval(feed: &ChangeFeed, busy: bool) -> bool {
    let started = tokio::time::Instant::now();
    let within = super::SWEEP_INTERVAL + Duration::from_secs(1);
    tokio::time::timeout(within, wait_for_mail(feed, busy, feed.generation()))
        .await
        .is_ok()
        && started.elapsed() >= super::SWEEP_INTERVAL
}

#[tokio::test]
async fn outbox_wait_while_busy_returns_after_the_sweep_interval() {
    assert!(ticks_on_the_interval(&fed_feed(), true).await);
}

#[tokio::test]
async fn outbox_wait_while_idle_and_nothing_changes_keeps_waiting() {
    let feed = fed_feed();
    assert!(!finishes(&feed, false, feed.generation()).await);
}

#[tokio::test]
async fn outbox_wait_while_idle_and_mail_changes_returns() {
    for table in super::WATCHED {
        let feed = fed_feed();
        let since = feed.generation();
        feed.record(table, Some("recipient"));
        assert!(finishes(&feed, false, since).await, "{table:?}");
    }
}

#[tokio::test]
async fn outbox_wait_while_idle_and_an_unrelated_table_changes_keeps_waiting() {
    let feed = fed_feed();
    let since = feed.generation();
    feed.record(RowChangedTable::Requests, Some("recipient"));
    assert!(!finishes(&feed, false, since).await);
}

#[tokio::test]
async fn outbox_wait_without_a_change_feed_returns_after_the_sweep_interval() {
    assert!(ticks_on_the_interval(&ChangeFeed::default(), false).await);
}
