use std::time::Duration;

use tokio::time::Instant;

use super::{FOLLOW_BACKSTOP, FollowGate, TickReport};
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

fn noted(feed: &ChangeFeed, following: bool, at: Instant) -> FollowGate {
    let mut gate = FollowGate::default();
    let report = TickReport {
        following,
        ..TickReport::default()
    };
    gate.note(Some(feed), Some(feed.generation()), &report, at);
    gate
}

#[tokio::test]
async fn follow_gate_before_any_tick_is_due() {
    assert!(FollowGate::default().due(Some(&fed()), Instant::now()));
}

#[tokio::test]
async fn follow_gate_after_a_quiet_follow_waits_while_no_repair_or_problem_changes() {
    let feed = fed();
    let now = Instant::now();
    let gate = noted(&feed, false, now);
    feed.record(RowChangedTable::Requests, None);
    assert!(!gate.due(Some(&feed), now + Duration::from_secs(3600)));
}

#[tokio::test]
async fn follow_gate_is_due_once_a_repair_changes() {
    let feed = fed();
    let now = Instant::now();
    let gate = noted(&feed, true, now);
    feed.record(RowChangedTable::WrightRepairs, None);
    assert!(gate.due(Some(&feed), now));
}

#[tokio::test]
async fn follow_gate_with_a_deployed_repair_waits_for_its_problem_to_change() {
    let feed = fed();
    let now = Instant::now();
    let gate = noted(&feed, true, now);
    assert!(!gate.due(Some(&feed), now + Duration::from_secs(120)));
    feed.record(RowChangedTable::WrightProblems, None);
    assert!(gate.due(Some(&feed), now + Duration::from_secs(120)));
}

#[tokio::test]
async fn follow_gate_with_a_repair_still_followed_is_due_after_the_backstop() {
    let feed = fed();
    let now = Instant::now();
    let gate = noted(&feed, true, now);
    assert!(gate.due(Some(&feed), now + FOLLOW_BACKSTOP));
}

#[tokio::test]
async fn follow_gate_after_a_resync_is_due() {
    let feed = fed();
    let now = Instant::now();
    let gate = noted(&feed, false, now);
    feed.resync();
    assert!(gate.due(Some(&feed), now));
}

#[tokio::test]
async fn follow_gate_without_a_fed_feed_follows_every_tick() {
    let feed = fed();
    let mut gate = FollowGate::default();
    let now = Instant::now();
    gate.note(None, Some(feed.generation()), &TickReport::default(), now);
    assert!(gate.due(None, now));
}
