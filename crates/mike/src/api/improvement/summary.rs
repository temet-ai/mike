use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

use crate::config::AppState;
use crate::store::persistence::improvement::summary::ImprovementSummary;

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct SummaryView {
    pub candidates_waiting: i64,
    pub directive_proposals_waiting: i64,
    pub guard_wakes_unlabelled: i64,
    pub guard_wakes_7d: i64,
    pub guard_appends_7d: i64,
    pub lessons_approved_7d: i64,
    pub lessons_rejected_7d: i64,
    pub proposals_decided_7d: i64,
    pub proposals_approved_7d: i64,
    pub proposals_rejected_7d: i64,
}

impl From<ImprovementSummary> for SummaryView {
    fn from(s: ImprovementSummary) -> Self {
        SummaryView {
            candidates_waiting: s.candidates_waiting,
            directive_proposals_waiting: s.directive_proposals_waiting,
            guard_wakes_unlabelled: s.guard_wakes_unlabelled,
            guard_wakes_7d: s.guard_wakes_7d,
            guard_appends_7d: s.guard_appends_7d,
            lessons_approved_7d: s.lessons_approved_7d,
            lessons_rejected_7d: s.lessons_rejected_7d,
            proposals_decided_7d: s.proposals_decided_7d,
            proposals_approved_7d: s.proposals_approved_7d,
            proposals_rejected_7d: s.proposals_rejected_7d,
        }
    }
}

pub(crate) async fn get_counts(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.improvement_counts().await {
        Ok(summary) => Json(SummaryView::from(summary)).into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
