use std::time::Duration;

use super::usage::{LlmRole, ROLE_COUNT, RoleSpend};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleShares {
    pct: [u32; ROLE_COUNT],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoleSharePcts {
    pub steerer: u32,
    pub session_judge: u32,
    pub judge_worker: u32,
    pub lesson_relations: u32,
    pub lesson_proposer: u32,
    pub eval: u32,
    pub other: u32,
}

impl RoleShares {
    pub const fn from_pcts(p: RoleSharePcts) -> Self {
        let mut pct = [0; ROLE_COUNT];
        pct[LlmRole::Steerer.slot()] = p.steerer;
        pct[LlmRole::SessionJudge.slot()] = p.session_judge;
        pct[LlmRole::JudgeWorker.slot()] = p.judge_worker;
        pct[LlmRole::LessonRelations.slot()] = p.lesson_relations;
        pct[LlmRole::LessonProposer.slot()] = p.lesson_proposer;
        pct[LlmRole::Eval.slot()] = p.eval;
        pct[LlmRole::Unattributed.slot()] = p.other;
        Self { pct }
    }

    #[cfg(test)]
    pub fn with_pct(mut self, role: LlmRole, pct: u32) -> Self {
        if let Some(slot) = self.pct.get_mut(role.index()) {
            *slot = pct;
        }
        self
    }

    pub fn pct(&self, role: LlmRole) -> u32 {
        self.pct.get(role.index()).copied().unwrap_or(0)
    }

    fn denominator(&self) -> u64 {
        u64::from(self.pct.iter().sum::<u32>().max(100))
    }

    pub(crate) fn guaranteed_calls(&self, role: LlmRole, limit: u32) -> u32 {
        let scaled = u64::from(limit) * u64::from(self.pct(role)) / self.denominator();
        u32::try_from(scaled).unwrap_or(limit)
    }

    pub(crate) fn guaranteed_usd(&self, role: LlmRole, ceiling_usd: f64) -> f64 {
        ceiling_usd * f64::from(self.pct(role)) / self.denominator() as f64
    }
}

impl Default for RoleShares {
    fn default() -> Self {
        Self::from_pcts(RoleSharePcts {
            steerer: 25,
            session_judge: 20,
            judge_worker: 25,
            lesson_relations: 5,
            lesson_proposer: 10,
            eval: 0,
            other: 10,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub calls_now: u32,
    pub window_remaining_calls: u32,
    pub next_slot_in: Duration,
}

impl Budget {
    pub fn exhausted(next_slot_in: Duration) -> Self {
        Self {
            calls_now: 0,
            window_remaining_calls: 0,
            next_slot_in,
        }
    }

    #[cfg(test)]
    pub fn is_exhausted(&self) -> bool {
        self.calls_now == 0
    }
}

pub(crate) fn calls_available(
    limit: u32,
    shares: &RoleShares,
    counts: &[u32; ROLE_COUNT],
    role: LlmRole,
) -> u32 {
    let guaranteed_total: u32 = LlmRole::ALL
        .iter()
        .map(|r| shares.guaranteed_calls(*r, limit))
        .sum();
    let shared_pool = limit.saturating_sub(guaranteed_total);
    let borrowed: u32 = LlmRole::ALL
        .iter()
        .map(|r| counts[r.index()].saturating_sub(shares.guaranteed_calls(*r, limit)))
        .sum();
    let own_remaining = shares
        .guaranteed_calls(role, limit)
        .saturating_sub(counts[role.index()]);
    let total_used: u32 = counts.iter().sum();
    own_remaining
        .saturating_add(shared_pool.saturating_sub(borrowed))
        .min(limit.saturating_sub(total_used))
}

const USD_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpendRefusal {
    pub(crate) spent_usd: f64,
    pub(crate) share_usd: f64,
    pub(crate) ceiling_usd: f64,
}

pub(crate) fn spend_refusal(
    ceiling_usd: f64,
    shares: &RoleShares,
    spent: &RoleSpend,
    role: LlmRole,
) -> Option<SpendRefusal> {
    if ceiling_usd <= 0.0 {
        return None;
    }
    let slice = shares.guaranteed_usd(role, ceiling_usd);
    let own = spent.of(role);
    if spent.total() >= ceiling_usd {
        return Some(SpendRefusal {
            spent_usd: own,
            share_usd: slice,
            ceiling_usd,
        });
    }
    if own < slice {
        return None;
    }
    let guaranteed_total: f64 = LlmRole::ALL
        .iter()
        .map(|r| shares.guaranteed_usd(*r, ceiling_usd))
        .sum();
    let shared_pool = (ceiling_usd - guaranteed_total).max(0.0);
    let borrowed: f64 = LlmRole::ALL
        .iter()
        .map(|r| (spent.of(*r) - shares.guaranteed_usd(*r, ceiling_usd)).max(0.0))
        .sum();
    if shared_pool - borrowed > USD_EPSILON {
        return None;
    }
    Some(SpendRefusal {
        spent_usd: own,
        share_usd: slice,
        ceiling_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(
        steerer: u32,
        session: u32,
        worker: u32,
        lesson: u32,
        other: u32,
    ) -> [u32; ROLE_COUNT] {
        [steerer, session, worker, lesson, 0, 0, other]
    }

    #[test]
    fn default_shares_include_lesson_proposer() {
        let shares = RoleShares::default();
        assert_eq!(shares.pct(LlmRole::Steerer), 25);
        assert_eq!(shares.pct(LlmRole::SessionJudge), 20);
        assert_eq!(shares.pct(LlmRole::JudgeWorker), 25);
        assert_eq!(shares.pct(LlmRole::LessonRelations), 5);
        assert_eq!(shares.pct(LlmRole::LessonProposer), 10);
        assert_eq!(shares.pct(LlmRole::Eval), 0);
        assert_eq!(shares.pct(LlmRole::Unattributed), 10);
    }

    #[test]
    fn eval_role_inside_its_share_is_not_refused_when_pool_is_gone() {
        let shares = RoleShares::default()
            .with_pct(LlmRole::Eval, 10)
            .with_pct(LlmRole::Unattributed, 5);
        let mut full = counts(25, 20, 25, 5, 0);
        full[LlmRole::Unattributed.index()] = 15;
        assert_eq!(calls_available(100, &shares, &full, LlmRole::Eval), 10);
    }

    #[test]
    fn default_guarantees_leave_five_percent_shared() {
        let shares = RoleShares::default();
        for (ceiling, expected) in [(0.70, 0.21), (5.00, 1.50)] {
            let reach = shares.guaranteed_usd(LlmRole::Steerer, ceiling) + ceiling * 0.05;
            assert!((reach - expected).abs() < 1e-9, "{ceiling}: {reach}");
        }
        assert!((shares.guaranteed_usd(LlmRole::Eval, 5.0)).abs() < 1e-12);
    }

    #[test]
    fn with_pct_sets_one_role_and_leaves_the_rest() {
        let shares = RoleShares::default().with_pct(LlmRole::Eval, 15);
        assert_eq!(shares.pct(LlmRole::Eval), 15);
        assert_eq!(shares.pct(LlmRole::Steerer), 25);
    }

    #[test]
    fn guarantees_are_floored_and_never_sum_past_the_limit() {
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 30,
            session_judge: 30,
            judge_worker: 30,
            lesson_relations: 10,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        for limit in [1, 7, 12, 30, 100, 600, 100_000] {
            let sum: u32 = LlmRole::ALL
                .iter()
                .map(|r| shares.guaranteed_calls(*r, limit))
                .sum();
            assert!(sum <= limit, "limit {limit}: guarantees sum to {sum}");
        }
    }

    #[test]
    fn oversubscribed_shares_are_scaled_down_proportionally() {
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 100,
            session_judge: 100,
            judge_worker: 100,
            lesson_relations: 100,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        let sum: u32 = LlmRole::ALL
            .iter()
            .map(|r| shares.guaranteed_calls(*r, 400))
            .sum();
        assert_eq!(sum, 400);
        assert_eq!(shares.guaranteed_calls(LlmRole::Steerer, 400), 100);
    }

    #[test]
    fn role_inside_its_guarantee_has_room_even_when_the_shared_pool_is_gone() {
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 50,
            session_judge: 50,
            judge_worker: 0,
            lesson_relations: 0,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        let full = counts(50, 30, 0, 0, 0);
        assert_eq!(
            calls_available(100, &shares, &full, LlmRole::SessionJudge),
            20
        );
        assert_eq!(calls_available(100, &shares, &full, LlmRole::Steerer), 0);
    }

    #[test]
    fn borrowing_draws_only_from_the_pool_beyond_all_guarantees() {
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 25,
            session_judge: 25,
            judge_worker: 25,
            lesson_relations: 5,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        let steerer_saturated = counts(25, 0, 0, 0, 0);
        assert_eq!(
            calls_available(100, &shares, &steerer_saturated, LlmRole::Steerer),
            20
        );
        let borrowed_all = counts(45, 0, 0, 0, 0);
        assert_eq!(
            calls_available(100, &shares, &borrowed_all, LlmRole::Steerer),
            0
        );
        assert_eq!(
            calls_available(100, &shares, &borrowed_all, LlmRole::JudgeWorker),
            25
        );
    }

    #[test]
    fn unattributed_role_only_ever_uses_the_shared_pool() {
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 25,
            session_judge: 25,
            judge_worker: 25,
            lesson_relations: 25,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        assert_eq!(
            calls_available(100, &shares, &counts(0, 0, 0, 0, 0), LlmRole::Unattributed),
            0
        );
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 20,
            session_judge: 20,
            judge_worker: 20,
            lesson_relations: 0,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        assert_eq!(
            calls_available(100, &shares, &counts(0, 0, 0, 0, 0), LlmRole::Unattributed),
            40
        );
    }

    #[test]
    fn spend_refusal_is_none_when_the_ceiling_is_disabled() {
        let shares = RoleShares::default();
        let mut spent = RoleSpend::default();
        spent.add(LlmRole::Steerer, 1_000_000.0);
        assert_eq!(spend_refusal(0.0, &shares, &spent, LlmRole::Steerer), None);
    }

    #[test]
    fn spend_refusal_names_the_role_spend_its_share_and_the_whole_ceiling() {
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 50,
            session_judge: 50,
            judge_worker: 0,
            lesson_relations: 0,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        let mut spent = RoleSpend::default();
        spent.add(LlmRole::Steerer, 0.5);
        let refusal = spend_refusal(1.0, &shares, &spent, LlmRole::Steerer);
        assert!(
            matches!(refusal, Some(r) if (r.spent_usd - 0.5).abs() < 1e-9
                && (r.share_usd - 0.5).abs() < 1e-9
                && (r.ceiling_usd - 1.0).abs() < 1e-9),
            "got {refusal:?}"
        );
        assert_eq!(
            spend_refusal(1.0, &shares, &spent, LlmRole::SessionJudge),
            None
        );
    }

    #[test]
    fn spend_refusal_at_own_share_with_pool_borrowed_by_another_role_reports_all_three_numbers() {
        let shares = RoleShares::from_pcts(RoleSharePcts {
            steerer: 30,
            session_judge: 30,
            judge_worker: 0,
            lesson_relations: 0,
            lesson_proposer: 0,
            eval: 0,
            other: 0,
        });
        let mut spent = RoleSpend::default();
        spent.add(LlmRole::SessionJudge, 0.3);
        spent.add(LlmRole::Steerer, 0.7);
        let refusal = spend_refusal(1.0, &shares, &spent, LlmRole::SessionJudge);
        assert!(
            matches!(refusal, Some(r) if (r.spent_usd - 0.3).abs() < 1e-9
                && (r.share_usd - 0.3).abs() < 1e-9
                && (r.ceiling_usd - 1.0).abs() < 1e-9),
            "got {refusal:?}"
        );
    }
}

#[cfg(test)]
mod proposer_share_tests {
    use super::*;

    #[test]
    fn proposer_has_guaranteed_capacity_when_evaluation_oversubscribes_shares() {
        let shares = RoleShares::default().with_pct(LlmRole::Eval, 20);
        assert!(calls_available(100, &shares, &[0; ROLE_COUNT], LlmRole::LessonProposer) > 0);
        assert_eq!(
            spend_refusal(
                10.0,
                &shares,
                &RoleSpend::default(),
                LlmRole::LessonProposer
            ),
            None
        );
        let mut spent = RoleSpend::default();
        spent.add(LlmRole::Eval, 10.0);
        assert!(spend_refusal(10.0, &shares, &spent, LlmRole::LessonProposer).is_some());
    }
}
