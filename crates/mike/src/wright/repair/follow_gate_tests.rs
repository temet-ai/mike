use super::{FollowGate, TickReport};
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;

fn fed() -> ChangeFeed {
    let feed = ChangeFeed::new();
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    spawn_change_feed(
        &feed,
        None,
        rx,
        tokio_util::sync::CancellationToken::new(),
        &tokio_util::task::TaskTracker::new(),
    );
    feed
}

fn quiet(feed: &ChangeFeed) -> FollowGate {
    let mut gate = FollowGate::default();
    gate.note(Some(feed), Some(feed.generation()), &TickReport::default());
    gate
}

#[tokio::test]
async fn follow_gate_before_any_tick_is_due() {
    assert!(FollowGate::default().due(Some(&fed())));
}

#[tokio::test]
async fn follow_gate_after_a_quiet_follow_waits_while_no_repair_changes() {
    let feed = fed();
    let gate = quiet(&feed);
    feed.record(RowChangedTable::Requests, None);
    assert!(!gate.due(Some(&feed)));
}

#[tokio::test]
async fn follow_gate_after_a_quiet_follow_is_due_once_a_repair_changes() {
    let feed = fed();
    let gate = quiet(&feed);
    feed.record(RowChangedTable::WrightRepairs, None);
    assert!(gate.due(Some(&feed)));
}

#[tokio::test]
async fn follow_gate_after_a_resync_is_due() {
    let feed = fed();
    let gate = quiet(&feed);
    feed.resync();
    assert!(gate.due(Some(&feed)));
}

#[tokio::test]
async fn follow_gate_while_a_repair_is_still_followed_stays_due() {
    let feed = fed();
    let mut gate = FollowGate::default();
    let report = TickReport {
        following: true,
        ..TickReport::default()
    };
    gate.note(Some(&feed), Some(feed.generation()), &report);
    assert!(gate.due(Some(&feed)));
}

#[tokio::test]
async fn follow_gate_without_a_fed_feed_follows_every_tick() {
    let feed = fed();
    let mut gate = FollowGate::default();
    gate.note(None, Some(feed.generation()), &TickReport::default());
    assert!(gate.due(None));
}
