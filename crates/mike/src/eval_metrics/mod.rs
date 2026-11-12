mod schedule;
mod settings;

#[cfg(test)]
mod schedule_tests;
#[cfg(test)]
mod settings_tests;

use chrono::Utc;
use tokio::time::{MissedTickBehavior, interval};

use crate::logged::ok_or_warn;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_metrics::{
    AlarmKind, AlarmThresholds, EpisodeChange, RollupRequest,
};

pub(crate) use schedule::day_start;
pub use schedule::{EVAL_ROLLUP_SQL_VERSION, rollup_due};
pub use settings::{EvalSettings, RoleInputs, expected_roles_from, resolve_eval_settings};

pub fn thresholds(s: &EvalSettings) -> AlarmThresholds {
    AlarmThresholds {
        mix_min_turns: s.mix_min_turns,
        mix_min_tvd: s.mix_min_tvd,
        mix_min_chi2: s.mix_min_chi2,
        backlog_min_turns: s.backlog_min_turns,
        stalled_min_waiting: s.stalled_min_waiting,
        stalled_hours: s.stalled_hours,
        refusals_min_share: s.refusals_min_share,
        refusals_min_refused: s.refusals_min_refused,
        gate_grace_hours: s.gate_grace_hours,
        call_loop_min_calls: s.call_loop_min_calls,
        call_loop_max_calls_per_row: s.call_loop_max_calls_per_row,
        session_judge_max_attempts_per_day:
            crate::store::persistence::judge::DEFAULT_MAX_ATTEMPTS_PER_DAY,
        band_enabled: s.band_enabled,
        band_min: s.band_min,
        band_max: s.band_max,
        shadow_disagreement_enabled: s.shadow_disagreement_enabled,
        shadow_disagreement_min_n: s.shadow_disagreement_min_n,
        shadow_disagreement_min_kappa: s.shadow_disagreement_min_kappa,
        metric_drop_enabled: s.metric_drop_enabled,
        metric_drop_min_n: s.metric_drop_min_n,
    }
}

fn log_changes(changes: &[EpisodeChange]) {
    for change in changes {
        if change.opened {
            log_change(change, "opened");
        }
        if change.cleared {
            log_change(change, "cleared");
        }
    }
}

fn log_change(change: &EpisodeChange, what: &'static str) {
    tracing::info!(id = change.id, kind = %change.kind, subject = %change.subject, change = what, "eval_metrics: alarm state changed");
}

async fn operational_pass(
    persist: &PersistHandle,
    t: &AlarmThresholds,
    roles: &[String],
) -> Option<()> {
    let firings = ok_or_warn(
        persist.evaluate_operational_alarms(t, roles).await,
        "eval_metrics.operational_alarms",
    )?;
    let changes = ok_or_warn(
        persist
            .apply_alarm_pass(
                &AlarmKind::OPERATIONAL,
                &firings,
                schedule::CLEAR_AFTER_QUIET_EVALUATIONS,
            )
            .await,
        "eval_metrics.alarm_upsert",
    )?;
    log_changes(&changes);
    Some(())
}

async fn daily_pass(persist: &PersistHandle, s: &EvalSettings, t: &AlarmThresholds) -> Option<()> {
    let last = ok_or_warn(
        persist.latest_rollup_day(EVAL_ROLLUP_SQL_VERSION).await,
        "eval_metrics.rollup_lookup",
    )?;
    let day = schedule::rollup_due(Utc::now(), s.rollup_hour_utc, last)?;
    let request = RollupRequest {
        for_day: day,
        mode: "scheduled",
        sql_version: EVAL_ROLLUP_SQL_VERSION,
        execute: true,
    };
    let report = ok_or_warn(
        persist.run_eval_rollup(request).await,
        "eval_metrics.rollup",
    )?;
    tracing::info!(for_day = %report.for_day, families = ?report.per_family, "eval_metrics: rollup written");
    let firings = ok_or_warn(
        persist.evaluate_daily_alarms(t, day_start(day)).await,
        "eval_metrics.daily_alarms",
    )?;
    let changes = ok_or_warn(
        persist
            .apply_alarm_pass(
                &AlarmKind::DAILY,
                &firings,
                schedule::CLEAR_AFTER_QUIET_EVALUATIONS,
            )
            .await,
        "eval_metrics.daily_alarm_upsert",
    )?;
    log_changes(&changes);
    Some(())
}

async fn action_effect_pass(persist: &PersistHandle, s: &EvalSettings) -> Option<()> {
    let last = ok_or_warn(
        persist.latest_completed_action_effect_day().await,
        "eval_metrics.action_effect_lookup",
    )?;
    let now = Utc::now();
    let day = schedule::rollup_due(now, s.rollup_hour_utc, last)?;
    let id = ok_or_warn(
        persist
            .current_or_claimed_action_effect_rollup_id(day)
            .await,
        "eval_metrics.action_effect_claim",
    )?;
    let rows = ok_or_warn(
        persist.write_action_effect_rows(id, now).await,
        "eval_metrics.action_effect_write",
    )?;
    tracing::info!(for_day = %day, rows, "eval_metrics: action effects written");
    Some(())
}

pub async fn run_tick(persist: &PersistHandle, roles: &[String], was_disabled: bool) -> bool {
    let Some(rows) = ok_or_warn(persist.eval_setting_rows().await, "eval_metrics.settings") else {
        return was_disabled;
    };
    let s = resolve_eval_settings(&rows);
    if !s.rollup_enabled {
        log_disabled(was_disabled);
        return true;
    }
    let t = thresholds(&s);
    operational_pass(persist, &t, roles).await;
    daily_pass(persist, &s, &t).await;
    action_effect_pass(persist, &s).await;
    false
}

fn log_disabled(was_disabled: bool) {
    if !was_disabled {
        tracing::info!("eval_metrics: disabled by eval.rollup.enabled, skipping ticks");
    }
}

pub fn spawn_eval_metrics_worker(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("eval_metrics: no persistence — not spawning");
        return;
    };
    let shutdown = state.shutdown();
    let tracker = state.tracker.clone();
    let worker_state = state.clone();
    tracker.spawn(async move {
        let mut ticker = interval(schedule::PASS_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut disabled = false;
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let roles: Vec<String> = settings::expected_roles(&worker_state)
                        .into_iter()
                        .map(str::to_owned)
                        .collect();
                    disabled = run_tick(&persist, &roles, disabled).await;
                }
            }
        }
    });
    tracing::info!(
        interval_secs = schedule::PASS_INTERVAL.as_secs(),
        "eval_metrics: worker spawned"
    );
}
