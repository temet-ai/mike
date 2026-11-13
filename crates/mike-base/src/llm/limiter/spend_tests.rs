use super::tests::{HOUR, LedgerSink, MINUTE, admit_n, limits};
use super::*;
use crate::llm::budget::RoleSharePcts;

#[test]
fn refusal_does_not_spend_the_other_counter() -> anyhow::Result<()> {
    let limiter = CallLimiter::new(1, MINUTE, 1000, MINUTE);
    limiter.admit(LlmRole::Unattributed, None)?;
    assert!(limiter.admit(LlmRole::Unattributed, None).is_err());
    assert!(limiter.admit(LlmRole::Unattributed, None).is_err());

    let snapshot = limiter.snapshot();
    assert_eq!(snapshot.calls_in_ceiling_window, 1);
    assert_eq!(snapshot.rate_refusals_total, 2);
    Ok(())
}

#[test]
fn refusals_are_counted_by_kind_and_role_without_spending_capacity() {
    let limiter = CallLimiter::new(2, MINUTE, 1000, HOUR);
    let now = Instant::now();
    admit_n(&limiter, LlmRole::JudgeWorker, 2, now);
    for _ in 0..3 {
        assert!(limiter.admit_at(LlmRole::JudgeWorker, None, now).is_err());
    }
    let snapshot = limiter.snapshot();
    assert_eq!(snapshot.rate_refusals_total, 3);
    assert_eq!(snapshot.refusals_by_role[LlmRole::JudgeWorker.index()], 3);
    assert_eq!(snapshot.refusals_by_role[LlmRole::Steerer.index()], 0);
    assert_eq!(snapshot.calls_this_minute, 2);
}

#[test]
fn role_at_its_spend_slice_is_refused_while_a_role_with_unused_spend_still_calls() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 50,
        session_judge: 50,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(LimiterLimits {
        daily_spend_ceiling_usd: 1.0,
        ..limits(1000, 1000, shares)
    });
    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 0.5)])));
    let now = Instant::now();

    let refused = limiter.admit_at(LlmRole::Steerer, None, now);
    assert!(
        matches!(
            refused,
            Err(LlmError::SpendCeilingExceeded { spent_usd, share_usd, ceiling_usd })
                if (spent_usd - 0.5).abs() < 1e-9
                    && (share_usd - 0.5).abs() < 1e-9
                    && (ceiling_usd - 1.0).abs() < 1e-9
        ),
        "expected the steerer's slice to refuse it, got {refused:?}"
    );
    assert!(limiter.admit_at(LlmRole::SessionJudge, None, now).is_ok());
    assert_eq!(limiter.snapshot().spend_refusals_total, 1);
}

#[test]
fn spend_slice_survives_another_role_borrowing_the_shared_pool() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 30,
        session_judge: 30,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(LimiterLimits {
        daily_spend_ceiling_usd: 1.0,
        ..limits(1000, 1000, shares)
    });
    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 0.7)])));
    let now = Instant::now();

    assert!(matches!(
        limiter.admit_at(LlmRole::Steerer, None, now),
        Err(LlmError::SpendCeilingExceeded { .. })
    ));
    for _ in 0..10 {
        assert!(limiter.admit_at(LlmRole::SessionJudge, None, now).is_ok());
    }
    assert_eq!(limiter.snapshot().spend_refusals_total, 1);
}

#[test]
fn role_may_borrow_the_unreserved_spend_pool_until_it_is_gone() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 30,
        session_judge: 30,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(LimiterLimits {
        daily_spend_ceiling_usd: 1.0,
        ..limits(1000, 1000, shares)
    });
    let now = Instant::now();

    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 0.5)])));
    assert!(limiter.admit_at(LlmRole::Steerer, None, now).is_ok());

    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 0.7)])));
    assert!(limiter.admit_at(LlmRole::Steerer, None, now).is_err());
}

#[test]
fn spend_ceiling_of_zero_never_refuses() {
    let limiter = CallLimiter::new(1000, MINUTE, 1000, HOUR);
    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 1_000_000.0)])));
    assert!(limiter.admit(LlmRole::Steerer, None).is_ok());
}

#[test]
fn spend_check_precedes_the_call_ceiling() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 100,
        session_judge: 0,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(LimiterLimits {
        daily_spend_ceiling_usd: 1.0,
        ..limits(1000, 0, shares)
    });
    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 1.0)])));
    assert!(matches!(
        limiter.admit(LlmRole::Steerer, None),
        Err(LlmError::SpendCeilingExceeded { .. })
    ));
}

#[test]
fn spend_blocked_role_has_no_budget_and_a_recheck_time() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 50,
        session_judge: 50,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(LimiterLimits {
        daily_spend_ceiling_usd: 1.0,
        ..limits(1000, 1000, shares)
    });
    limiter.set_spend_source(Some(LedgerSink::with(&[(LlmRole::Steerer, 0.5)])));
    let budget = limiter.available(LlmRole::Steerer);
    assert!(budget.is_exhausted());
    assert!(budget.next_slot_in > Duration::ZERO);
}
