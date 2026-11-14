use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde_json::{Value, json};

use crate::lesson_proposer::full_pass::{FullPass, run_full_pass};
use crate::lesson_proposer::{ProposerDeps, ProposerSettings};
use crate::memory_workers::{self, LESSON_PROPOSER, Skip};
use crate::quiescence_worker::proposer_tick::{lesson_client, semantic_config};
use crate::state::AppState;

#[cfg(test)]
#[path = "proposer_pass_tests.rs"]
mod tests;

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

pub(crate) fn ran_body(pass: &FullPass) -> Value {
    json!({
        "outcome": "ran",
        "proposer": pass.proposer,
        "topics": pass.topics,
        "skill_author": pass.skill_author,
        "skill_eval": pass.skill_eval,
    })
}

pub(crate) async fn post_proposer_pass(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return refuse(StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled");
    };
    let now = Utc::now();
    if !state.feature_toggle_overlay.load().judge_lessons {
        memory_workers::record_skip(LESSON_PROPOSER, now, Skip::Off);
        return refuse(
            StatusCode::CONFLICT,
            "the judge_lessons feature toggle is off; turn it on in /settings to run the lesson proposer",
        );
    }
    let settings = ProposerSettings::load(persist).await;
    let Some(llm) = lesson_client(
        &state.config().steerer,
        &settings.effective_model,
        state.llm_usage_sink.as_ref(),
        &state.steerer_openrouter_key,
    ) else {
        memory_workers::record_skip(LESSON_PROPOSER, now, Skip::NoClient);
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "no lesson proposer model client: set an OpenRouter key for the steerer, or choose a model in judge_lessons.model this proxy can call",
        );
    };
    let semantic = semantic_config(persist).await;
    let env = state.decision_env();
    let deps = ProposerDeps {
        persist,
        llm: &llm,
        embedding: &state.embedding_service,
        semantic: &semantic,
        settings: &settings,
        decisions: env.as_ref(),
    };
    memory_workers::mark_running(LESSON_PROPOSER, Utc::now());
    let pass = run_full_pass(&deps).await;
    memory_workers::record_ran(LESSON_PROPOSER, Utc::now(), &pass.counts());
    Json(ran_body(&pass)).into_response()
}
