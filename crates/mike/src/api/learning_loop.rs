use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;

pub(crate) mod assemble;
pub(crate) mod stages;
#[cfg(test)]
mod tests;

use assemble::{LearningLoop, StageInput, assemble};
use stages::{RERANK_TUNING_KEY, STAGES, StageDef, Switch};

use crate::api::handlers::loop_job;
use crate::feature_toggles::get_toggle_enabled;
use crate::memory_workers;
use crate::state::AppState;
use crate::store::persistence::learning_loop::LoopCounts;
use crate::store::persistence::web_repo::WebPersist as _;

const WINDOW_HOURS: i64 = 24;

fn refuse(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

pub(crate) async fn get_learning_loop(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence is disabled, so no stage of the learning loop can run".to_owned(),
        );
    };
    let now = Utc::now();
    let since = now - Duration::hours(WINDOW_HOURS);
    match persist.web().learning_loop_counts(since).await {
        Ok(counts) => Json(build(&state, &counts, since, now)).into_response(),
        Err(error) => refuse(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not count what the learning loop wrote since {since}: {error}"),
        ),
    }
}

pub(crate) fn build(
    state: &AppState,
    counts: &LoopCounts,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> LearningLoop {
    let inputs = STAGES
        .iter()
        .map(|def| stage_input(state, def, counts, now))
        .collect();
    assemble(since, inputs)
}

fn stage_input(
    state: &AppState,
    def: &'static StageDef,
    counts: &LoopCounts,
    now: DateTime<Utc>,
) -> StageInput {
    let rows = def.rows.map(|rows| rows(counts));
    let mut input = StageInput {
        def,
        toggle_key: None,
        enabled: true,
        last_ran_at: rows.and_then(|(_, at)| at),
        running_since: None,
        last_outcome: None,
        produced_24h: rows.map(|(produced, _)| produced),
        failing: false,
    };
    match def.switch {
        Switch::Always => {}
        Switch::Toggle(key) => {
            input.toggle_key = Some(key);
            let toggles = state.feature_toggle_overlay.load();
            input.enabled = get_toggle_enabled(&toggles, key).unwrap_or(false);
        }
        Switch::RerankTuning => {
            input.toggle_key = Some(RERANK_TUNING_KEY);
            input.enabled = state.rerank_service().current_config().tuning_enabled;
        }
        Switch::Job(name) => apply_job(&mut input, name, now),
    }
    if let Some(worker) = def.worker {
        apply_worker(&mut input, worker);
    }
    input
}

fn apply_worker(input: &mut StageInput, worker: &str) {
    let status = memory_workers::snapshot(worker);
    input.last_ran_at = status.last_ran_at.max(input.last_ran_at);
    input.running_since = status.running_since;
    input.last_outcome = status
        .last_outcome
        .and_then(|outcome| serde_json::to_value(outcome).ok());
}

fn apply_job(input: &mut StageInput, name: &str, now: DateTime<Utc>) {
    let Some(job) = loop_job(name, now) else {
        return;
    };
    let view = job.view;
    input.toggle_key = Some(job.enabled_key);
    input.enabled = view.enabled;
    input.last_ran_at = view.last_finished_at.max(input.last_ran_at);
    input.running_since = view.running_since;
    input.failing = job.failing;
    input.last_outcome = Some(json!({
        "state": view.state,
        "exit_code": view.last_exit_code,
        "detail": view.detail,
    }));
}
