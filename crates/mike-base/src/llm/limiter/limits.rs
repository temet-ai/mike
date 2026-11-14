use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;

use crate::llm::budget::{RoleSharePcts, RoleShares};
use crate::settings::SettingsOverlay;

const DEFAULT_RATE_LIMIT_PER_MINUTE: u32 = 12;
const DEFAULT_CEILING_MAX_CALLS: u32 = 100;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const DEFAULT_CEILING_WINDOW: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimiterLimits {
    pub rate_per_window: u32,
    pub rate_window: Duration,
    pub ceiling_max_calls: u32,
    pub ceiling_window: Duration,
    pub daily_spend_ceiling_usd: f64,
    pub shares: RoleShares,
}

impl LimiterLimits {
    pub fn compiled_defaults() -> Self {
        Self::fixed(
            DEFAULT_RATE_LIMIT_PER_MINUTE,
            RATE_WINDOW,
            DEFAULT_CEILING_MAX_CALLS,
            DEFAULT_CEILING_WINDOW,
        )
    }

    pub fn fixed(
        rate_per_window: u32,
        rate_window: Duration,
        ceiling_max_calls: u32,
        ceiling_window: Duration,
    ) -> Self {
        Self {
            rate_per_window,
            rate_window,
            ceiling_max_calls,
            ceiling_window,
            daily_spend_ceiling_usd: 0.0,
            shares: RoleShares::default(),
        }
    }

    pub fn for_decisions(overlay: &SettingsOverlay) -> Self {
        let n = |v: i64| u32::try_from(v.max(1)).unwrap_or(1);
        Self {
            daily_spend_ceiling_usd: overlay.decisions_daily_spend_ceiling_usd.max(0.0),
            shares: RoleShares::from_pcts(RoleSharePcts {
                decision: 100,
                ..RoleSharePcts::default()
            }),
            ..Self::fixed(
                n(overlay.decisions_rate_per_minute),
                RATE_WINDOW,
                n(overlay.decisions_max_calls_per_hour),
                DEFAULT_CEILING_WINDOW,
            )
        }
    }

    pub fn from_overlay(overlay: &SettingsOverlay) -> Self {
        let share = |pct: i64| u32::try_from(pct).unwrap_or(0);
        Self {
            rate_per_window: u32::try_from(overlay.llm_rate_limit_per_minute).unwrap_or(0),
            rate_window: RATE_WINDOW,
            ceiling_max_calls: u32::try_from(overlay.llm_ceiling_max_calls).unwrap_or(0),
            ceiling_window: Duration::from_secs(
                u64::try_from(overlay.llm_ceiling_window_secs).unwrap_or(0),
            ),
            daily_spend_ceiling_usd: overlay.llm_daily_spend_ceiling_usd,
            shares: RoleShares::from_pcts(RoleSharePcts {
                steerer: share(overlay.llm_budget_steerer_share_pct),
                session_judge: share(overlay.llm_budget_session_judge_share_pct),
                judge_worker: share(overlay.llm_budget_judge_worker_share_pct),
                lesson_relations: share(overlay.llm_budget_lesson_relations_share_pct),
                lesson_proposer: share(overlay.llm_budget_lesson_proposer_share_pct),
                eval: share(overlay.llm_budget_eval_share_pct),
                other: share(overlay.llm_budget_other_share_pct),
                decision: share(overlay.llm_budget_decision_share_pct),
            }),
        }
    }
}

pub enum LimitSource {
    Fixed(LimiterLimits),
    Overlay(Arc<ArcSwap<SettingsOverlay>>),
}

impl LimitSource {
    pub fn resolve(&self) -> LimiterLimits {
        match self {
            Self::Fixed(limits) => *limits,
            Self::Overlay(overlay) => LimiterLimits::from_overlay(&overlay.load()),
        }
    }
}
