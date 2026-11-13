use crate::store::persistence::lessons_repo::LessonsPersist as _;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{error, persist_failed, unavailable};
use crate::state::AppState;
use crate::store::persistence::lessons::skill_review_writes::{InstallReport, ReportedInstall};

const RETIRED_REASONS: [&str; 4] = ["unloaded", "corrected_above_base", "user", "policy"];

#[derive(Debug, Deserialize)]
pub struct InstallsBody {
    machine_uuid: Uuid,
    machine_name: String,
    installs: Vec<ReportedInstall>,
}

fn invalid(body: &InstallsBody) -> Option<&'static str> {
    if body.machine_name.trim().is_empty() {
        return Some("machine_name is required");
    }
    let bad_reason = body.installs.iter().any(|i| {
        i.retired_reason
            .as_deref()
            .is_some_and(|r| !RETIRED_REASONS.contains(&r))
    });
    if bad_reason {
        return Some("retired_reason must be unloaded, corrected_above_base, user or policy");
    }
    body.installs
        .iter()
        .any(|i| i.folder.trim().is_empty() || i.path.trim().is_empty())
        .then_some("every install needs a folder and a path")
}

pub async fn report_installs(
    State(state): State<AppState>,
    Json(body): Json<InstallsBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    if let Some(message) = invalid(&body) {
        return error(StatusCode::BAD_REQUEST, message);
    }
    let report = InstallReport {
        machine_uuid: body.machine_uuid,
        machine_name: &body.machine_name,
        installs: &body.installs,
    };
    match persist.lessons().record_skill_installs(report).await {
        Ok(written) => Json(json!({ "recorded": written })).into_response(),
        Err(e) => persist_failed(&e),
    }
}
