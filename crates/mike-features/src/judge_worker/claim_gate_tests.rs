use chrono::{Duration, Utc};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::{CLAIM_BACKSTOP, ClaimGate, IMMEDIATE_SETTLE, note_claim, queue_unchanged, settle_for};
use crate::change_feed::{ChangeFeed, spawn_change_feed};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::acquire_ephemeral_db;

const BUNDLE: i64 = 94;

fn emptied(now: chrono::DateTime<Utc>) -> ClaimGate {
    let mut gate = ClaimGate::default();
    gate.settle(BUNDLE, 0, true, now);
    gate
}

#[test]
fn claim_gate_first_sweep_claims() {
    let mut gate = ClaimGate::default();
    assert!(gate.should_claim(BUNDLE, true, Duration::zero(), Utc::now()));
}

#[test]
fn claim_gate_after_an_empty_claim_with_no_change_skips_until_the_backstop() {
    let now = Utc::now();
    let mut gate = emptied(now);
    assert!(!gate.should_claim(BUNDLE, true, Duration::zero(), now + Duration::seconds(599)));
    assert!(gate.should_claim(BUNDLE, true, Duration::zero(), now + CLAIM_BACKSTOP));
}

#[test]
fn claim_gate_after_a_queue_change_claims_at_once_without_settle() {
    let now = Utc::now();
    let mut gate = emptied(now);
    assert!(gate.should_claim(BUNDLE, false, Duration::zero(), now + Duration::seconds(60)));
}

#[test]
fn claim_gate_with_immediate_judging_waits_the_settle_from_the_first_seen_change() {
    let now = Utc::now();
    let mut gate = emptied(now);
    let seen = now + Duration::seconds(60);
    assert!(!gate.should_claim(BUNDLE, false, IMMEDIATE_SETTLE, seen));
    assert!(!gate.should_claim(
        BUNDLE,
        false,
        IMMEDIATE_SETTLE,
        seen + Duration::seconds(240)
    ));
    assert!(gate.should_claim(BUNDLE, false, IMMEDIATE_SETTLE, seen + IMMEDIATE_SETTLE));
}

#[test]
fn claim_gate_on_another_bundle_claims() {
    let now = Utc::now();
    let mut gate = emptied(now);
    assert!(gate.should_claim(BUNDLE + 1, true, Duration::zero(), now));
}

#[test]
fn claim_gate_after_a_claim_that_found_turns_claims_again() {
    let now = Utc::now();
    let mut gate = ClaimGate::default();
    gate.settle(BUNDLE, 3, true, now);
    assert!(gate.should_claim(BUNDLE, true, Duration::zero(), now));
}

#[test]
fn claim_gate_without_a_change_feed_claims_every_sweep() {
    let now = Utc::now();
    let mut gate = ClaimGate::default();
    gate.settle(BUNDLE, 0, false, now);
    assert!(gate.should_claim(BUNDLE, false, IMMEDIATE_SETTLE, now));
}

#[tokio::test]
async fn queue_unchanged_after_an_empty_claim_until_a_judge_queue_notice() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let feed = ChangeFeed::new();
    let (_events, rx) = broadcast::channel(16);
    let shutdown = CancellationToken::new();
    spawn_change_feed(
        &feed,
        Some(&persist),
        rx,
        shutdown.clone(),
        &TaskTracker::new(),
    );
    let now = Utc::now();
    assert!(!queue_unchanged(&persist, now));
    note_claim(&persist, persist.fresh_ticket(), 0);
    assert!(queue_unchanged(&persist, now));
    feed.record(RowChangedTable::Requests, None);
    assert!(queue_unchanged(&persist, now));
    feed.record(RowChangedTable::JudgeQueue, None);
    assert!(!queue_unchanged(&persist, now));
    note_claim(&persist, persist.fresh_ticket(), 0);
    assert!(queue_unchanged(&persist, now));
    note_claim(&persist, persist.fresh_ticket(), 2);
    assert!(!queue_unchanged(&persist, now));
    shutdown.cancel();
    db.cleanup().await
}

#[test]
fn settle_for_is_only_set_when_the_immediate_judge_runs() {
    assert_eq!(settle_for(true), IMMEDIATE_SETTLE);
    assert_eq!(settle_for(false), Duration::zero());
}
