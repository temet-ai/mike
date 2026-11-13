use super::tests::{HOUR, LedgerSink, MINUTE, admit_n, limits};
use super::*;
use crate::llm::budget::RoleSharePcts;

#[test]
fn reservation_charges_up_front_and_its_calls_are_not_charged_twice() -> anyhow::Result<()> {
    let limiter = Arc::new(CallLimiter::new(20, MINUTE, 100, HOUR));
    let reservation = limiter.reserve(LlmRole::JudgeWorker, 4);
    assert_eq!(reservation.granted(), 4);
    assert_eq!(limiter.snapshot().calls_this_minute, 4);

    for _ in 0..4 {
        limiter.admit(LlmRole::JudgeWorker, Some(&reservation))?;
    }
    assert_eq!(reservation.remaining(), 0);
    assert_eq!(limiter.snapshot().calls_this_minute, 4);

    limiter.admit(LlmRole::JudgeWorker, Some(&reservation))?;
    assert_eq!(limiter.snapshot().calls_this_minute, 5);
    Ok(())
}

#[test]
fn reservation_is_capped_at_what_the_role_can_actually_have() {
    let limiter = Arc::new(CallLimiter::new(3, MINUTE, 100, HOUR));
    let reservation = limiter.reserve(LlmRole::Steerer, 25);
    assert_eq!(reservation.granted(), 3);
}

#[test]
fn dropping_a_reservation_releases_its_unused_slots() {
    let limiter = Arc::new(CallLimiter::new(20, MINUTE, 100, HOUR));
    {
        let reservation = limiter.reserve(LlmRole::JudgeWorker, 6);
        assert_eq!(limiter.snapshot().calls_this_minute, 6);
        assert!(
            limiter
                .admit(LlmRole::JudgeWorker, Some(&reservation))
                .is_ok()
        );
    }
    let snapshot = limiter.snapshot();
    assert_eq!(snapshot.calls_this_minute, 1);
    assert_eq!(snapshot.calls_in_ceiling_window, 1);
}

#[test]
fn a_reservation_holds_capacity_against_other_roles() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 50,
        session_judge: 50,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        decision: 0,
        eval: 0,
        other: 0,
    });
    let limiter = Arc::new(CallLimiter::with_limits(limits(10, 1000, shares)));
    let reservation = limiter.reserve(LlmRole::Steerer, 10);
    assert_eq!(reservation.granted(), 5);
    let now = Instant::now();
    assert_eq!(limiter.available_at(LlmRole::Steerer, now).calls_now, 0);
    assert_eq!(
        limiter.available_at(LlmRole::SessionJudge, now).calls_now,
        5
    );
}

#[test]
fn reservation_from_an_expired_window_is_not_honoured() {
    let limiter = Arc::new(CallLimiter::new(2, MINUTE, 100, HOUR));
    let reservation = limiter.reserve(LlmRole::Steerer, 2);
    let after_roll = Instant::now() + MINUTE;
    assert!(
        limiter
            .admit_at(LlmRole::Steerer, Some(&reservation), after_roll)
            .is_ok()
    );
    assert_eq!(reservation.remaining(), 2);
    assert_eq!(
        limiter.snapshot().calls_this_minute_by_role[LlmRole::Steerer.index()],
        1
    );
}

#[test]
fn reservation_for_another_role_is_ignored() {
    let limiter = Arc::new(CallLimiter::new(10, MINUTE, 100, HOUR));
    let reservation = limiter.reserve(LlmRole::Steerer, 3);
    assert!(
        limiter
            .admit(LlmRole::SessionJudge, Some(&reservation))
            .is_ok()
    );
    assert_eq!(reservation.remaining(), 3);
    assert_eq!(limiter.snapshot().calls_this_minute, 4);
}

#[test]
fn reserve_on_an_exhausted_role_grants_nothing_and_says_when_to_retry() {
    let limiter = Arc::new(CallLimiter::new(1, MINUTE, 100, HOUR));
    let now = Instant::now();
    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 1, now), 1);
    let reservation = limiter.reserve(LlmRole::Steerer, 5);
    assert_eq!(reservation.granted(), 0);
    assert!(reservation.next_slot_in() > Duration::ZERO);
    assert_eq!(limiter.snapshot().rate_refusals_total, 0);
}

#[test]
fn reservation_still_honours_the_spend_slice_per_call() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 50,
        session_judge: 50,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        decision: 0,
        eval: 0,
        other: 0,
    });
    let limiter = Arc::new(CallLimiter::with_limits(LimiterLimits {
        daily_spend_ceiling_usd: 1.0,
        ..limits(1000, 1000, shares)
    }));
    let reservation = limiter.reserve(LlmRole::Steerer, 5);
    assert_eq!(reservation.granted(), 5);
    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 0.5)])));
    assert!(matches!(
        limiter.admit(LlmRole::Steerer, Some(&reservation)),
        Err(LlmError::SpendCeilingExceeded { .. })
    ));
}
