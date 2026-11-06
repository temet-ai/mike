use serde::Serialize;

use crate::llm::CallUsage;

pub(crate) const FIRST_ITEM_ESTIMATE_USD: f64 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct SpendLimits {
    pub(crate) ceiling_usd: f64,
    pub(crate) eval_allowance_usd: f64,
    pub(crate) run_max_usd: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(crate) enum BudgetPause {
    GlobalCeiling {
        spent_usd: f64,
        estimate_usd: f64,
        ceiling_usd: f64,
    },
    EvalAllowance {
        spent_usd: f64,
        estimate_usd: f64,
        allowance_usd: f64,
    },
    RunMax {
        spent_usd: f64,
        estimate_usd: f64,
        max_usd: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SharePcts {
    pub(crate) eval: u32,
    pub(crate) total: u32,
}

pub(crate) fn eval_allowance_usd(ceiling_usd: f64, shares: SharePcts) -> f64 {
    let pool = 100_u32.saturating_sub(shares.total);
    let denominator = f64::from(shares.total.max(100));
    ceiling_usd * f64::from(shares.eval + pool) / denominator
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EvalSpendGuard {
    limits: SpendLimits,
    run_spent_usd: f64,
    judged: u32,
}

impl EvalSpendGuard {
    pub(crate) fn new(limits: SpendLimits, already_spent_usd: f64, already_judged: u32) -> Self {
        Self {
            limits,
            run_spent_usd: already_spent_usd,
            judged: already_judged,
        }
    }

    pub(crate) fn estimate_usd(&self) -> f64 {
        if self.judged == 0 {
            FIRST_ITEM_ESTIMATE_USD
        } else {
            self.run_spent_usd / f64::from(self.judged)
        }
    }

    pub(crate) fn check(&self, today_all_usd: f64, today_eval_usd: f64) -> Result<(), BudgetPause> {
        let estimate = self.estimate_usd();
        let l = self.limits;
        if self.run_spent_usd + estimate > l.run_max_usd {
            return Err(BudgetPause::RunMax {
                spent_usd: self.run_spent_usd,
                estimate_usd: estimate,
                max_usd: l.run_max_usd,
            });
        }
        if l.ceiling_usd > 0.0 && today_all_usd + estimate > l.ceiling_usd {
            return Err(BudgetPause::GlobalCeiling {
                spent_usd: today_all_usd,
                estimate_usd: estimate,
                ceiling_usd: l.ceiling_usd,
            });
        }
        if l.ceiling_usd > 0.0 && today_eval_usd + estimate > l.eval_allowance_usd {
            return Err(BudgetPause::EvalAllowance {
                spent_usd: today_eval_usd,
                estimate_usd: estimate,
                allowance_usd: l.eval_allowance_usd,
            });
        }
        Ok(())
    }

    pub(crate) fn record(&mut self, usage: &CallUsage) {
        self.run_spent_usd += usage.cost_usd.unwrap_or(0.0);
        self.judged += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{BudgetPause, EvalSpendGuard, SharePcts, SpendLimits, eval_allowance_usd};
    use crate::llm::CallUsage;

    fn limits(ceiling: f64, allowance: f64, max: f64) -> SpendLimits {
        SpendLimits {
            ceiling_usd: ceiling,
            eval_allowance_usd: allowance,
            run_max_usd: max,
        }
    }

    fn usage(cost: f64) -> CallUsage {
        CallUsage {
            cost_usd: Some(cost),
            ..CallUsage::default()
        }
    }

    #[test]
    fn spend_guard_pauses_when_global_ceiling_would_be_crossed() {
        let guard = EvalSpendGuard::new(limits(5.0, 5.0, 10.0), 0.0, 0);
        assert!(matches!(
            guard.check(4.999, 0.0),
            Err(BudgetPause::GlobalCeiling { .. })
        ));
        assert!(guard.check(1.0, 0.0).is_ok());
    }

    #[test]
    fn spend_guard_pauses_when_eval_allowance_would_be_crossed() {
        let guard = EvalSpendGuard::new(limits(5.0, 0.75, 10.0), 0.0, 0);
        assert!(matches!(
            guard.check(1.0, 0.749),
            Err(BudgetPause::EvalAllowance { .. })
        ));
    }

    #[test]
    fn spend_guard_pauses_at_run_max_usd_even_when_ceiling_is_zero() {
        let guard = EvalSpendGuard::new(limits(0.0, 0.0, 0.01), 0.0099, 3);
        assert!(matches!(
            guard.check(0.0, 0.0),
            Err(BudgetPause::RunMax { .. })
        ));
    }

    #[test]
    fn spend_guard_checks_per_item_so_room_for_five_judges_five() {
        let mut guard = EvalSpendGuard::new(limits(0.0, 0.0, 0.0100001), 0.0, 0);
        let mut judged = 0;
        while guard.check(0.0, 0.0).is_ok() {
            guard.record(&usage(0.002));
            judged += 1;
        }
        assert_eq!(judged, 5);
    }

    #[test]
    fn allowance_is_the_eval_share_plus_the_unguaranteed_pool() {
        let allowance = eval_allowance_usd(5.0, SharePcts { eval: 0, total: 85 });
        assert!((allowance - 0.75).abs() < 1e-9);
        let raised = eval_allowance_usd(
            5.0,
            SharePcts {
                eval: 10,
                total: 95,
            },
        );
        assert!((raised - 0.75).abs() < 1e-9);
    }
}
