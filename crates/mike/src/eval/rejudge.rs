use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::settings::SettingsOverlay;
use crate::store::persistence::eval_rejudge::{
    NewRejudgeRun, RejudgeFinish, RejudgeProgress, RejudgeRunRow, RejudgeWindow,
};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, thiserror::Error)]
pub enum RejudgeError {
    #[error(
        "turn_judge.live_bundle_id is 0: no bundle is live, so there is nothing to re-judge with; promote a gated bundle first"
    )]
    NoLiveBundle,
    #[error(
        "bundle {0} has no passing holdout gate in eval.gates; gate it with mike eval gate before a re-judge"
    )]
    Ungated(i64),
    #[error("--until {until} is not after --since {since}")]
    EmptyWindow {
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    },
    #[error(
        "the turn-judge history re-judge must be done first, so done-v1 can read projected reactions"
    )]
    TurnRejudgeNotDone,
    #[error(transparent)]
    Persist(#[from] PersistError),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct JudgeBudget {
    pub ceiling_usd: f64,
    pub slice_usd: f64,
    pub pool_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejudgePlan {
    pub window: RejudgeWindow,
    pub gate_id: i64,
    pub daily_cap: i32,
    pub progress: RejudgeProgress,
    pub mean_cost_usd: Option<f64>,
    pub estimated_calls: i64,
    pub estimated_usd: Option<f64>,
    pub days_at_cap: i64,
    pub budget: JudgeBudget,
}

pub fn judge_budget(overlay: &SettingsOverlay) -> JudgeBudget {
    let ceiling = overlay.llm_daily_spend_ceiling_usd;
    let shares = [
        overlay.llm_budget_steerer_share_pct,
        overlay.llm_budget_session_judge_share_pct,
        overlay.llm_budget_judge_worker_share_pct,
        overlay.llm_budget_lesson_relations_share_pct,
        overlay.llm_budget_eval_share_pct,
        overlay.llm_budget_other_share_pct,
    ];
    let pct = |v: i64| v.clamp(0, 100) as f64 / 100.0;
    let pool = (100 - shares.iter().sum::<i64>()).max(0);
    JudgeBudget {
        ceiling_usd: ceiling,
        slice_usd: ceiling * pct(overlay.llm_budget_judge_worker_share_pct),
        pool_usd: ceiling * pct(pool),
    }
}

pub fn daily_cap(overlay: &SettingsOverlay, override_cap: Option<i32>) -> i32 {
    override_cap
        .unwrap_or_else(|| i32::try_from(overlay.turn_judge_rejudge_daily_call_cap).unwrap_or(300))
        .max(1)
}

pub async fn plan_rejudge(
    persist: &PersistHandle,
    overlay: &SettingsOverlay,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    override_cap: Option<i32>,
) -> Result<RejudgePlan, RejudgeError> {
    let bundle_id = overlay.turn_judge_live_bundle_id;
    if bundle_id <= 0 {
        return Err(RejudgeError::NoLiveBundle);
    }
    if until <= since {
        return Err(RejudgeError::EmptyWindow { since, until });
    }
    let gate_id = persist
        .passing_holdout_gate(bundle_id)
        .await?
        .ok_or(RejudgeError::Ungated(bundle_id))?;
    let window = RejudgeWindow {
        bundle_id,
        since,
        until,
    };
    let progress = persist.rejudge_progress(&window).await?;
    let mean_cost_usd = persist.rejudge_mean_cost_usd(bundle_id).await?;
    let daily_cap = daily_cap(overlay, override_cap);
    let estimated_calls = progress.remaining;
    Ok(RejudgePlan {
        window,
        gate_id,
        daily_cap,
        estimated_usd: mean_cost_usd.map(|c| c * estimated_calls as f64),
        mean_cost_usd,
        estimated_calls,
        days_at_cap: progress.days_at_cap(daily_cap),
        progress,
        budget: judge_budget(overlay),
    })
}

pub async fn start_rejudge(
    persist: &PersistHandle,
    plan: &RejudgePlan,
    created_by: &str,
) -> Result<i64, RejudgeError> {
    let run = NewRejudgeRun {
        window: plan.window,
        daily_call_cap: plan.daily_cap,
        estimated_calls: i32::try_from(plan.estimated_calls).unwrap_or(i32::MAX),
        estimated_usd: plan.estimated_usd,
        created_by,
        role: "turn_judge",
    };
    Ok(persist.create_rejudge_run(&run).await?)
}

pub async fn cancel_active(persist: &PersistHandle) -> Result<Option<RejudgeRunRow>, RejudgeError> {
    let Some(run) = persist.active_rejudge_run().await? else {
        return Ok(None);
    };
    persist
        .finish_rejudge_run(run.id, RejudgeFinish::Cancelled)
        .await?;
    Ok(Some(run))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionRejudgePlan {
    pub window: RejudgeWindow,
    pub eligible: i64,
    pub daily_cap: i32,
    pub estimated_usd: Option<f64>,
    pub days_at_cap: i64,
}

async fn turn_rejudge_done(persist: &PersistHandle) -> Result<bool, RejudgeError> {
    let runs = persist.list_rejudge_runs().await?;
    Ok(runs
        .iter()
        .any(|r| r.role == "turn_judge" && r.status == "done"))
}

pub async fn plan_session_rejudge(
    persist: &PersistHandle,
    overlay: &SettingsOverlay,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    override_cap: Option<i32>,
) -> Result<SessionRejudgePlan, RejudgeError> {
    let bundle_id = overlay.session_judge_live_bundle_id;
    if bundle_id <= 0 || overlay.turn_judge_live_bundle_id <= 0 {
        return Err(RejudgeError::NoLiveBundle);
    }
    if !turn_rejudge_done(persist).await? {
        return Err(RejudgeError::TurnRejudgeNotDone);
    }
    let model = persist
        .get_bundle(bundle_id)
        .await?
        .and_then(|b| {
            b.components
                .get("model")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let window = crate::store::persistence::session_end::SessionRejudgeWindow {
        bundle_id,
        since,
        until,
        judge_model: &model,
    };
    let eligible = i64::try_from(
        persist
            .session_rejudge_candidates(&window, 1_000_000)
            .await?
            .len(),
    )
    .unwrap_or(i64::MAX);
    let daily_cap = override_cap
        .unwrap_or_else(|| {
            i32::try_from(overlay.session_judge_rejudge_daily_call_cap).unwrap_or(200)
        })
        .max(1);
    let mean = persist.session_v3_mean_cost(bundle_id).await?;
    Ok(SessionRejudgePlan {
        window: RejudgeWindow {
            bundle_id,
            since,
            until,
        },
        eligible,
        daily_cap,
        estimated_usd: mean.map(|m| m * eligible as f64),
        days_at_cap: (eligible + i64::from(daily_cap) - 1) / i64::from(daily_cap),
    })
}

pub async fn start_session_rejudge(
    persist: &PersistHandle,
    plan: &SessionRejudgePlan,
    created_by: &str,
) -> Result<i64, RejudgeError> {
    let run = NewRejudgeRun {
        window: plan.window,
        daily_call_cap: plan.daily_cap,
        estimated_calls: i32::try_from(plan.eligible).unwrap_or(i32::MAX),
        estimated_usd: plan.estimated_usd,
        created_by,
        role: "session_judge",
    };
    Ok(persist.create_rejudge_run(&run).await?)
}

#[cfg(test)]
mod tests {
    use super::{daily_cap, judge_budget};
    use crate::settings::SettingsOverlay;

    #[test]
    fn the_judge_slice_and_pool_follow_the_shares() {
        let overlay = SettingsOverlay {
            llm_daily_spend_ceiling_usd: 5.0,
            ..SettingsOverlay::default()
        };
        let budget = judge_budget(&overlay);
        assert!((budget.slice_usd - 1.25).abs() < 1e-9);
        assert!((budget.pool_usd - 0.75).abs() < 1e-9);
    }

    #[test]
    fn the_cli_cap_overrides_the_setting() {
        let overlay = SettingsOverlay::default();
        assert_eq!(daily_cap(&overlay, None), 300);
        assert_eq!(daily_cap(&overlay, Some(50)), 50);
        assert_eq!(daily_cap(&overlay, Some(0)), 1);
    }
}
