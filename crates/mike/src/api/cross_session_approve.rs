use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::change_log::current_build;
use crate::config::AppState;
use crate::store::persistence::change_log::NewChange;
use crate::store::persistence::cross_session::ApproveDirectiveProposal;
use crate::web_base::change_source::change_source_from_headers;

const VALID_KINDS: [&str; 4] = ["correction", "convention", "warning", "preference"];

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

#[derive(Debug, Deserialize)]
pub struct ApproveBody {
    pub text: String,
    pub kind: String,
    pub target_project_ids: Vec<i64>,
    pub expires_at: Option<DateTime<Utc>>,
}

fn validate(body: &ApproveBody) -> Option<Response> {
    if body.text.trim().is_empty() {
        return Some(error(
            StatusCode::BAD_REQUEST,
            "text must not be empty".to_owned(),
        ));
    }
    if !VALID_KINDS.contains(&body.kind.as_str()) {
        return Some(error(
            StatusCode::BAD_REQUEST,
            format!("kind must be one of {VALID_KINDS:?}"),
        ));
    }
    if body.target_project_ids.is_empty() {
        return Some(error(
            StatusCode::BAD_REQUEST,
            "target_project_ids must not be empty".to_owned(),
        ));
    }
    None
}

async fn log_approval(
    persist: &crate::store::persistence::PersistHandle,
    headers: &HeaderMap,
    id: i64,
    body: &ApproveBody,
    lessons: &[crate::store::persistence::cross_session::ApprovedLesson],
) {
    let source = change_source_from_headers(headers).to_setting_json(&current_build());
    let key = format!("proposal:{id}");
    let old = json!("open");
    let new = json!({
        "status": "approved",
        "kind": body.kind,
        "target_project_ids": body.target_project_ids,
        "lessons": lessons,
    });
    let change = persist
        .append_change(&NewChange {
            subject_kind: "directive_decision",
            subject_key: &key,
            op: "update",
            old_value: Some(&old),
            new_value: Some(&new),
            source_json: &source,
        })
        .await;
    if let Err(e) = change {
        tracing::warn!(proposal = id, error = %e, "directive proposal: approve change-log row failed");
    }
}

pub async fn approve_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ApproveBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    if let Some(resp) = validate(&body) {
        return resp;
    }
    let params = ApproveDirectiveProposal {
        id,
        text: &body.text,
        kind: &body.kind,
        target_project_ids: &body.target_project_ids,
        expires_at: body.expires_at,
    };
    let lessons = match persist.approve_directive_proposal(&params).await {
        Ok(rows) => rows,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    if lessons.is_empty() {
        let status = persist
            .fetch_proposal_detail(id)
            .await
            .ok()
            .flatten()
            .map(|d| d.proposal.status);
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "the proposal is not open", "status": status})),
        )
            .into_response();
    }
    log_approval(persist, &headers, id, &body, &lessons).await;
    Json(json!({"id": id, "status": "approved", "lessons": lessons})).into_response()
}
