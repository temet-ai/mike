use super::*;
use crate::llm::budget::RoleSharePcts;
use crate::llm::usage::{DailySpendLedger, LlmCallRecord};

pub const HOUR: Duration = Duration::from_secs(3600);
pub const MINUTE: Duration = Duration::from_secs(60);

pub fn limits(rate: u32, ceiling: u32, shares: RoleShares) -> LimiterLimits {
    LimiterLimits {
        shares,
        ..LimiterLimits::fixed(rate, MINUTE, ceiling, HOUR)
    }
}

pub fn admit_n(limiter: &CallLimiter, role: LlmRole, attempts: u32, now: Instant) -> u32 {
    (0..attempts)
        .filter(|_| limiter.admit_at(role, None, now).is_ok())
        .count() as u32
}

pub struct LedgerSink(DailySpendLedger);

impl LedgerSink {
    pub fn with(spend: &[(LlmRole, f64)]) -> Arc<Self> {
        let ledger = DailySpendLedger::seeded(RoleSpend::default());
        for (role, usd) in spend {
            ledger.add(*role, *usd);
        }
        Arc::new(Self(ledger))
    }
}

impl LlmUsageSink for LedgerSink {
    fn record(&self, call: LlmCallRecord) {
        if let Some(cost) = call.cost_usd {
            self.0.add(call.role, cost);
        }
    }

    fn spent_today(&self) -> RoleSpend {
        self.0.snapshot()
    }
}

#[test]
fn allows_calls_up_to_the_ceiling_then_refuses() -> anyhow::Result<()> {
    let limiter = CallLimiter::new(1000, MINUTE, 3, MINUTE);
    for _ in 0..3 {
        limiter.admit(LlmRole::Unattributed, None)?;
    }
    let err = limiter.admit(LlmRole::Unattributed, None);
    assert!(matches!(
        err,
        Err(LlmError::CeilingExceeded { limit: 3, .. })
    ));
    Ok(())
}

#[test]
fn allows_calls_up_to_the_rate_limit_then_refuses() -> anyhow::Result<()> {
    let limiter = CallLimiter::new(3, MINUTE, 1000, MINUTE);
    for _ in 0..3 {
        limiter.admit(LlmRole::Unattributed, None)?;
    }
    let err = limiter.admit(LlmRole::Unattributed, None);
    assert!(matches!(err, Err(LlmError::RateLimited { limit: 3 })));
    Ok(())
}

#[test]
fn rate_window_resets_after_it_elapses() {
    let limiter = CallLimiter::new(1, MINUTE, 1000, HOUR);
    let base = Instant::now();
    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 3, base), 1);
    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 3, base + MINUTE), 1);
}

#[test]
fn ceiling_window_resets_after_it_elapses() {
    let limiter = CallLimiter::new(1000, MINUTE, 1, HOUR);
    let base = Instant::now();
    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 3, base), 1);
    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 3, base + HOUR), 1);
}

#[test]
fn global_limiter_returns_the_same_shared_instance() {
    assert!(Arc::ptr_eq(&global_limiter(), &global_limiter()));
}

#[test]
fn overlay_defaults_resolve_to_the_compiled_limits() {
    assert_eq!(
        LimiterLimits::from_overlay(&SettingsOverlay::default()),
        LimiterLimits::compiled_defaults()
    );
}

#[test]
fn role_inside_its_share_is_never_refused_while_another_role_saturates() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 25,
        session_judge: 25,
        judge_worker: 25,
        lesson_relations: 5,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(limits(1000, 100, shares));
    let now = Instant::now();

    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 200, now), 45);
    assert_eq!(admit_n(&limiter, LlmRole::SessionJudge, 200, now), 25);
    assert_eq!(admit_n(&limiter, LlmRole::JudgeWorker, 200, now), 25);
    assert_eq!(admit_n(&limiter, LlmRole::LessonRelations, 200, now), 5);
    assert_eq!(admit_n(&limiter, LlmRole::Unattributed, 200, now), 0);

    let snapshot = limiter.snapshot();
    assert_eq!(snapshot.calls_in_ceiling_window, 100);
}

#[test]
fn role_saturating_the_shared_pool_cannot_touch_reserved_slots() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 25,
        session_judge: 25,
        judge_worker: 25,
        lesson_relations: 5,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(limits(1000, 100, shares));
    let now = Instant::now();

    assert_eq!(admit_n(&limiter, LlmRole::JudgeWorker, 500, now), 45);
    assert_eq!(limiter.available_at(LlmRole::Steerer, now).calls_now, 25);
    assert_eq!(
        limiter.available_at(LlmRole::SessionJudge, now).calls_now,
        25
    );
    assert_eq!(limiter.available_at(LlmRole::JudgeWorker, now).calls_now, 0);
}

#[test]
fn borrowed_capacity_is_reclaimed_when_the_window_rolls() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 25,
        session_judge: 25,
        judge_worker: 25,
        lesson_relations: 5,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(limits(1000, 100, shares));
    let base = Instant::now();

    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 500, base), 45);
    assert_eq!(limiter.available_at(LlmRole::Steerer, base).calls_now, 0);

    let later = base + HOUR;
    assert_eq!(limiter.available_at(LlmRole::Steerer, later).calls_now, 45);
    assert_eq!(
        limiter.available_at(LlmRole::JudgeWorker, later).calls_now,
        45
    );
}

#[test]
fn rate_window_applies_the_same_per_role_guarantee() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 50,
        session_judge: 50,
        judge_worker: 0,
        lesson_relations: 0,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    let limiter = CallLimiter::with_limits(limits(10, 1000, shares));
    let now = Instant::now();

    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 50, now), 5);
    assert_eq!(admit_n(&limiter, LlmRole::SessionJudge, 50, now), 5);
    assert_eq!(admit_n(&limiter, LlmRole::JudgeWorker, 50, now), 0);
}

#[test]
fn zero_budget_reports_zero_available_and_a_positive_next_slot() {
    let limiter = CallLimiter::new(2, MINUTE, 1000, HOUR);
    let now = Instant::now();
    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 5, now), 2);

    let budget = limiter.available_at(LlmRole::Steerer, now);
    assert_eq!(budget.calls_now, 0);
    assert!(
        budget.window_remaining_calls > 0,
        "the ceiling window still has room; only the per-minute rate is spent"
    );
    assert!(budget.next_slot_in > Duration::ZERO);
    assert!(budget.next_slot_in <= MINUTE);

    let after_roll = limiter.available_at(LlmRole::Steerer, now + MINUTE);
    assert_eq!(after_roll.calls_now, 2);
    assert_eq!(after_roll.next_slot_in, Duration::ZERO);
}

#[test]
fn next_slot_waits_for_the_ceiling_window_when_the_ceiling_is_the_binding_limit() {
    let limiter = CallLimiter::new(1000, MINUTE, 1, HOUR);
    let now = Instant::now();
    assert_eq!(admit_n(&limiter, LlmRole::Steerer, 5, now), 1);

    let budget = limiter.available_at(LlmRole::Steerer, now);
    assert_eq!(budget.calls_now, 0);
    assert_eq!(budget.next_slot_in, RECHECK_CAP);
}

#[test]
fn available_reports_calls_now_and_ceiling_headroom_separately() {
    let limiter = CallLimiter::new(3, MINUTE, 50, HOUR);
    let budget = limiter.available_at(LlmRole::Unattributed, Instant::now());
    assert_eq!(budget.calls_now, 3);
    assert_eq!(
        budget.window_remaining_calls, 9,
        "the ceiling window offers the role its 10% guarantee (5) plus the 4 calls no role is guaranteed"
    );
}

#[test]
fn available_predicts_exactly_how_many_admits_succeed() {
    let shares = RoleShares::from_pcts(RoleSharePcts {
        steerer: 25,
        session_judge: 25,
        judge_worker: 25,
        lesson_relations: 5,
        lesson_proposer: 0,
        eval: 0,
        other: 0,
    });
    for role in LlmRole::ALL {
        let limiter = CallLimiter::with_limits(limits(40, 100, shares));
        let now = Instant::now();
        let predicted = limiter.available_at(role, now).calls_now;
        assert_eq!(admit_n(&limiter, role, 500, now), predicted, "{role:?}");
    }
}

#[test]
fn live_overlay_limits_apply_to_a_role_with_no_runtime_settings() -> anyhow::Result<()> {
    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay {
        llm_ceiling_max_calls: 1000,
        llm_rate_limit_per_minute: 1000,
        ..SettingsOverlay::default()
    }));
    let limiter = CallLimiter::new(1, MINUTE, 1, HOUR);
    limiter.install_runtime(overlay.clone(), None);
    let now = Instant::now();

    assert_eq!(admit_n(&limiter, LlmRole::SessionJudge, 10, now), 10);

    overlay.store(Arc::new(SettingsOverlay {
        llm_ceiling_max_calls: 12,
        llm_rate_limit_per_minute: 1000,
        ..SettingsOverlay::default()
    }));
    let err = limiter.admit_at(LlmRole::SessionJudge, None, now);
    assert!(
        matches!(err, Err(LlmError::CeilingExceeded { limit: 12, .. })),
        "the lowered live ceiling must apply immediately, got {err:?}"
    );
    Ok(())
}

#[test]
fn live_overlay_shares_change_the_guarantee_immediately() {
    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay {
        llm_ceiling_max_calls: 100,
        llm_rate_limit_per_minute: 1000,
        llm_budget_steerer_share_pct: 10,
        llm_budget_session_judge_share_pct: 10,
        llm_budget_judge_worker_share_pct: 10,
        llm_budget_lesson_relations_share_pct: 10,
        llm_budget_other_share_pct: 10,
        ..SettingsOverlay::default()
    }));
    let limiter = CallLimiter::new(1, MINUTE, 1, HOUR);
    limiter.install_runtime(overlay.clone(), None);
    let now = Instant::now();
    assert_eq!(limiter.available_at(LlmRole::Steerer, now).calls_now, 50);

    overlay.store(Arc::new(SettingsOverlay {
        llm_ceiling_max_calls: 100,
        llm_rate_limit_per_minute: 1000,
        llm_budget_steerer_share_pct: 90,
        llm_budget_session_judge_share_pct: 0,
        llm_budget_judge_worker_share_pct: 0,
        llm_budget_lesson_relations_share_pct: 0,
        llm_budget_other_share_pct: 0,
        ..SettingsOverlay::default()
    }));
    assert_eq!(limiter.available_at(LlmRole::Steerer, now).calls_now, 90);
    assert_eq!(
        limiter.available_at(LlmRole::SessionJudge, now).calls_now,
        0
    );
}

#[test]
fn exhaust_and_reset_seams_cover_every_role() {
    let limiter = CallLimiter::new(12, MINUTE, 100, HOUR);
    limiter.exhaust_rate();
    for role in LlmRole::ALL {
        assert!(
            limiter.admit(role, None).is_err(),
            "{role:?} slipped past exhaust"
        );
    }
    limiter.reset();
    assert!(limiter.admit(LlmRole::Steerer, None).is_ok());
}

#[test]
fn proposer_share_overlay_applies_to_live_limiter() {
    let overlay = Arc::new(ArcSwap::from_pointee(SettingsOverlay {
        llm_ceiling_max_calls: 100,
        llm_rate_limit_per_minute: 1000,
        llm_budget_eval_share_pct: 20,
        llm_daily_spend_ceiling_usd: 10.0,
        ..SettingsOverlay::default()
    }));
    let limiter = CallLimiter::new(1000, MINUTE, 100, HOUR);
    limiter.install_runtime(overlay.clone(), None);
    let now = Instant::now();
    assert!(limiter.available_at(LlmRole::LessonProposer, now).calls_now > 0);
    let mut disabled = (**overlay.load()).clone();
    disabled.llm_budget_lesson_proposer_share_pct = 0;
    overlay.store(Arc::new(disabled));
    assert_eq!(
        limiter.available_at(LlmRole::LessonProposer, now).calls_now,
        0
    );
}
