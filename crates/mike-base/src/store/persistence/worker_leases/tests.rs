use std::time::{Duration, Instant};

use uuid::Uuid;

use super::{
    Decision, LeaseHolder, LeaseMemo, MIN_LEASE_TIMEOUT, RECHECK_FLOOR, Verdict, decision_is_fresh,
    lease_timeout, renew_after, wait_for_holder,
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
fn renew_after_any_timeout_is_half_of_it() {
    assert_eq!(renew_after(MIN_LEASE_TIMEOUT), Duration::from_secs(90));
    assert_eq!(
        renew_after(Duration::from_secs(900)),
        Duration::from_secs(450)
    );
}

#[test]
fn verdict_claimed_held_lease_is_kept_for_half_the_timeout() {
    assert_eq!(
        Verdict::claimed(true, MIN_LEASE_TIMEOUT),
        Verdict {
            held: true,
            fresh_for: Duration::from_secs(90),
        }
    );
}

#[test]
fn verdict_claimed_refused_lease_is_read_again_after_the_floor() {
    assert_eq!(
        Verdict::claimed(false, MIN_LEASE_TIMEOUT),
        Verdict {
            held: false,
            fresh_for: RECHECK_FLOOR,
        }
    );
    assert!(RECHECK_FLOOR >= Duration::from_secs(1));
}

fn holder(machine_uuid: Uuid, age_secs: u64) -> LeaseHolder {
    LeaseHolder {
        machine_uuid,
        age: Duration::from_secs(age_secs),
    }
}

#[test]
fn wait_for_holder_another_live_holder_waits_until_its_heartbeat_expires() {
    let other = holder(Uuid::new_v4(), 40);
    assert_eq!(
        wait_for_holder(Some(&other), Uuid::new_v4(), MIN_LEASE_TIMEOUT),
        Some(Verdict {
            held: false,
            fresh_for: Duration::from_secs(140),
        })
    );
}

#[test]
fn wait_for_holder_holder_about_to_expire_waits_at_least_the_floor() {
    let other = LeaseHolder {
        machine_uuid: Uuid::new_v4(),
        age: MIN_LEASE_TIMEOUT - Duration::from_millis(100),
    };
    assert_eq!(
        wait_for_holder(Some(&other), Uuid::new_v4(), MIN_LEASE_TIMEOUT),
        Some(Verdict {
            held: false,
            fresh_for: RECHECK_FLOOR,
        })
    );
}

#[test]
fn wait_for_holder_free_own_or_expired_lease_is_claimed() {
    let me = Uuid::new_v4();
    assert_eq!(wait_for_holder(None, me, MIN_LEASE_TIMEOUT), None);
    assert_eq!(
        wait_for_holder(Some(&holder(me, 40)), me, MIN_LEASE_TIMEOUT),
        None
    );
    assert_eq!(
        wait_for_holder(Some(&holder(Uuid::new_v4(), 180)), me, MIN_LEASE_TIMEOUT),
        None
    );
}

#[test]
fn decision_is_fresh_until_its_deadline() {
    let start = Instant::now();
    let decision = Decision {
        until: start + Duration::from_secs(90),
        held: true,
    };
    assert!(decision_is_fresh(decision, start));
    assert!(decision_is_fresh(
        decision,
        start + Duration::from_secs(90) - Duration::from_millis(1)
    ));
    assert!(!decision_is_fresh(
        decision,
        start + Duration::from_secs(90)
    ));
}

#[test]
fn memo_returns_the_remembered_verdict_only_for_its_fresh_period() {
    let memo = LeaseMemo::for_machine(Uuid::new_v4());
    let start = Instant::now();
    let waiting = Verdict {
        held: false,
        fresh_for: Duration::from_secs(140),
    };
    assert_eq!(memo.remembered("steerer", start), None);
    memo.remember("steerer", waiting, start);
    assert_eq!(memo.remembered("steerer", start), Some(false));
    assert_eq!(
        memo.remembered("steerer", start + Duration::from_secs(139)),
        Some(false)
    );
    assert_eq!(memo.remembered("session_judge", start), None);
    assert_eq!(
        memo.remembered("steerer", start + Duration::from_secs(140)),
        None
    );
}

#[test]
fn memo_for_machine_presets_the_machine_identity() {
    let machine = Uuid::new_v4();
    let memo = LeaseMemo::for_machine(machine);
    assert_eq!(memo.machine.get().copied(), Some(machine));
}

fn expired_at_once(held: bool) -> Option<Verdict> {
    Some(Verdict {
        held,
        fresh_for: Duration::ZERO,
    })
}

#[tokio::test]
async fn decide_after_an_expired_verdict_hands_its_answer_to_the_next_claim() -> anyhow::Result<()>
{
    let memo = LeaseMemo::for_machine(Uuid::new_v4());
    let mut seen = Vec::new();
    for held in [true, false, false] {
        let answer = memo
            .decide("handover", |last| {
                seen.push(last);
                async move { expired_at_once(held) }
            })
            .await;
        assert_eq!(answer, Some(held));
    }
    assert_eq!(seen, vec![None, Some(true), Some(false)]);
    Ok(())
}

async fn claims_once_while_callers_wait(outcome: Option<bool>) -> anyhow::Result<()> {
    const CALLERS: usize = 8;
    let memo = LeaseMemo::for_machine(Uuid::new_v4());
    let gate = memo.claim_gate("single_flight");
    let gate = &gate;
    let claims = std::sync::atomic::AtomicUsize::new(0);
    let claims = &claims;
    let callers = (0..CALLERS).map(|_| {
        memo.decide("single_flight", move |_| async move {
            claims.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            while std::sync::Arc::strong_count(gate) < CALLERS + 2 {
                tokio::task::yield_now().await;
            }
            outcome.map(|held| Verdict::claimed(held, MIN_LEASE_TIMEOUT))
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
    assert_eq!(memo.decide("retry", |_| async { None }).await, None);
    assert_eq!(
        memo.decide("retry", |_| async {
            Some(Verdict::claimed(false, MIN_LEASE_TIMEOUT))
        })
        .await,
        Some(false)
    );
    assert_eq!(
        memo.decide("retry", |_| async {
            Some(Verdict::claimed(true, MIN_LEASE_TIMEOUT))
        })
        .await,
        Some(false)
    );
    Ok(())
}
