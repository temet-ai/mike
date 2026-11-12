use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::api::eval::labels::spa_origin;
use crate::state::AppState;
use crate::store::persistence::PersistError;
use crate::store::persistence::lessons::placement_proposals::{
    PlacementDecision, PlacementProposalRow, ProposalProjectRow,
};

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; lesson proposals are not available",
    )
}

fn persist_failed(e: &PersistError) -> Response {
    error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[derive(Debug, Serialize)]
struct ProjectsResponse {
    projects: Vec<ProposalProjectRow>,
    undecided: i64,
}

pub(crate) async fn list_projects(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.list_proposal_projects().await {
        Ok(projects) => {
            let undecided = projects.iter().map(|p| p.undecided).sum();
            Json(ProjectsResponse {
                projects,
                undecided,
            })
            .into_response()
        }
        Err(e) => persist_failed(&e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectParams {
    project: Uuid,
}

#[derive(Debug, Serialize)]
struct ProposalsResponse {
    project: Uuid,
    proposals: Vec<PlacementProposalRow>,
}

pub(crate) async fn list_proposals(
    State(state): State<AppState>,
    Query(params): Query<ProjectParams>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.list_placement_proposals(params.project).await {
        Ok(proposals) => Json(ProposalsResponse {
            project: params.project,
            proposals,
        })
        .into_response(),
        Err(e) => persist_failed(&e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct DecisionBody {
    decision: String,
}

fn parse_decision(code: &str) -> Option<PlacementDecision> {
    match code {
        "accept" => Some(PlacementDecision::Accepted),
        "decline" => Some(PlacementDecision::Declined),
        "clear" => Some(PlacementDecision::Cleared),
        _ => None,
    }
}

pub(crate) async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(lesson_id): Path<i64>,
    Json(body): Json<DecisionBody>,
) -> Response {
    if spa_origin(&headers, &state).is_none() {
        return error(
            StatusCode::FORBIDDEN,
            "proposal decisions are made from the mike web app only",
        );
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let Some(decision) = parse_decision(&body.decision) else {
        return error(
            StatusCode::BAD_REQUEST,
            "decision must be accept, decline or clear",
        );
    };
    match persist.set_placement_decision(lesson_id, decision).await {
        Ok(true) => Json(json!({ "ok": true })).into_response(),
        Ok(false) => error(
            StatusCode::NOT_FOUND,
            "no rule or agent proposal with that id",
        ),
        Err(e) => persist_failed(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_codes_map_to_stored_decisions() {
        assert_eq!(parse_decision("accept"), Some(PlacementDecision::Accepted));
        assert_eq!(parse_decision("decline"), Some(PlacementDecision::Declined));
        assert_eq!(parse_decision("clear"), Some(PlacementDecision::Cleared));
    }

    #[test]
    fn unknown_decision_code_is_rejected() {
        assert_eq!(parse_decision("approve"), None);
    }
}
