use crate::store::persistence::lessons_repo::LessonsPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::state::Mode;
use super::{error, persist_failed, unavailable};
use crate::api::eval::labels::spa_origin;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::skill_review_writes::SkillReview;

const FROM_APP_ONLY: &str = "skill decisions are made from the mike web app only";
const MAX_SKILL_MD_BYTES: usize = 64 * 1024;

fn checked<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
) -> Result<&'a PersistHandle, Box<Response>> {
    if spa_origin(headers, state).is_none() {
        return Err(Box::new(error(StatusCode::FORBIDDEN, FROM_APP_ONLY)));
    }
    state.persist().ok_or_else(|| Box::new(unavailable()))
}

fn changed(found: Result<bool, crate::store::persistence::PersistError>, what: &str) -> Response {
    match found {
        Ok(true) => Json(json!({ "ok": true })).into_response(),
        Ok(false) => error(StatusCode::NOT_FOUND, format!("no such {what}")),
        Err(e) => persist_failed(&e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReviewBody {
    decision: String,
}

pub(crate) async fn review_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ReviewBody>,
) -> Response {
    let persist = match checked(&state, &headers) {
        Ok(persist) => persist,
        Err(resp) => return *resp,
    };
    let review = match body.decision.as_str() {
        "approve" => SkillReview::Approved,
        "reject" => SkillReview::Rejected,
        "clear" => SkillReview::Cleared,
        _ => {
            return error(
                StatusCode::BAD_REQUEST,
                "decision must be approve, reject or clear",
            );
        }
    };
    changed(
        persist.lessons().set_skill_draft_review(id, review).await,
        "skill draft",
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct ContentBody {
    skill_md: String,
}

pub(crate) async fn edit_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ContentBody>,
) -> Response {
    let persist = match checked(&state, &headers) {
        Ok(persist) => persist,
        Err(resp) => return *resp,
    };
    if body.skill_md.trim().is_empty() || body.skill_md.len() > MAX_SKILL_MD_BYTES {
        return error(
            StatusCode::BAD_REQUEST,
            "SKILL.md must be non-empty and at most 64 KiB",
        );
    }
    changed(
        persist.lessons().edit_skill_draft(id, &body.skill_md).await,
        "skill draft",
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct PolicyBody {
    project: Uuid,
    mode: String,
}

pub(crate) async fn set_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PolicyBody>,
) -> Response {
    let persist = match checked(&state, &headers) {
        Ok(persist) => persist,
        Err(resp) => return *resp,
    };
    if Mode::parse(&body.mode).is_none() {
        return error(StatusCode::BAD_REQUEST, "mode must be auto or ask");
    }
    changed(
        persist
            .lessons()
            .set_skill_policy(body.project, &body.mode, "skills page")
            .await,
        "project",
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct FolderBody {
    project: Uuid,
    folder: String,
}

pub(crate) async fn retire_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FolderBody>,
) -> Response {
    let persist = match checked(&state, &headers) {
        Ok(persist) => persist,
        Err(resp) => return *resp,
    };
    changed(
        persist
            .lessons()
            .retire_skill_folder(body.project, &body.folder)
            .await,
        "project",
    )
}

pub(crate) async fn restore_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FolderBody>,
) -> Response {
    let persist = match checked(&state, &headers) {
        Ok(persist) => persist,
        Err(resp) => return *resp,
    };
    changed(
        persist
            .lessons()
            .restore_skill_folder(body.project, &body.folder)
            .await,
        "retired skill",
    )
}
