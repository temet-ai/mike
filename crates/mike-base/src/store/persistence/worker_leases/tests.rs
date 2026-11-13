use std::time::{Duration, Instant};

use uuid::Uuid;

use super::{
    Decision, LeaseMemo, MIN_LEASE_TIMEOUT, RENEW_EVERY, decision_is_fresh, lease_timeout,
};

#[test]
fn lease_timeout_short_interval_uses_the_floor() {
    assert_eq!(lease_timeout(Duration::ZERO), MIN_LEASE_TIMEOUT);
    assert_eq!(lease_timeout(Duration::from_secs(30)), MIN_LEASE_TIMEOUT);
    assert_eq!(lease_timeout(Duration::from_secs(60)), MIN_LEASE_TIMEOUT);
}

#[test]
fn lease_timeout_long_interval_covers_three_ticks() {
    assert_eq!(
        lease_timeout(Duration::from_secs(300)),
        Duration::from_secs(900)
    );
}

#[test]
fn decision_is_fresh_until_the_renew_period_has_passed() {
    let start = Instant::now();
    let decision = Decision {
        at: start,
        held: true,
    };
    assert!(decision_is_fresh(decision, start));
    assert!(decision_is_fresh(
        decision,
        start + RENEW_EVERY - Duration::from_millis(1)
    ));
    assert!(!decision_is_fresh(decision, start + RENEW_EVERY));
}

#[test]
fn memo_returns_the_remembered_decision_only_while_fresh() {
    let memo = LeaseMemo::for_machine(Uuid::new_v4());
    let start = Instant::now();
    assert_eq!(memo.remembered("steerer", start), None);
    memo.remember("steerer", false, start);
    assert_eq!(memo.remembered("steerer", start), Some(false));
    assert_eq!(memo.remembered("session_judge", start), None);
    assert_eq!(memo.remembered("steerer", start + RENEW_EVERY), None);
}

#[test]
fn memo_for_machine_presets_the_machine_identity() {
    let machine = Uuid::new_v4();
    let memo = LeaseMemo::for_machine(machine);
    assert_eq!(memo.machine.get().copied(), Some(machine));
}

async fn claims_once_while_callers_wait(outcome: Option<bool>) -> anyhow::Result<()> {
    const CALLERS: usize = 8;
    let memo = LeaseMemo::for_machine(Uuid::new_v4());
    let gate = memo.claim_gate("single_flight");
    let gate = &gate;
    let claims = std::sync::atomic::AtomicUsize::new(0);
    let claims = &claims;
    let callers = (0..CALLERS).map(|_| {
        memo.decide("single_flight", move || async move {
            claims.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            while std::sync::Arc::strong_count(gate) < CALLERS + 2 {
                tokio::task::yield_now().await;
            }
            outcome
        })
    });
    let decisions = futures::future::join_all(callers).await;
    assert_eq!(claims.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(decisions, vec![outcome; CALLERS]);
    Ok(())
}

#[tokio::test]
async fn decide_with_concurrent_callers_claims_once_and_shares_the_result() -> anyhow::Result<()> {
    claims_once_while_callers_wait(Some(true)).await
}

#[tokio::test]
async fn decide_with_concurrent_callers_shares_a_failed_claim_without_retrying()
-> anyhow::Result<()> {
    claims_once_while_callers_wait(None).await
}

#[tokio::test]
async fn decide_after_a_failed_claim_claims_again_for_a_later_caller() -> anyhow::Result<()> {
    let memo = LeaseMemo::for_machine(Uuid::new_v4());
    assert_eq!(memo.decide("retry", || async { None }).await, None);
    assert_eq!(
        memo.decide("retry", || async { Some(false) }).await,
        Some(false)
    );
    assert_eq!(
        memo.decide("retry", || async { Some(true) }).await,
        Some(false)
    );
    Ok(())
}
