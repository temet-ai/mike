use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::state::{Mode, SkillState, needs_you, skill_state};
use super::{error, persist_failed, unavailable};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::skill_review::{SkillFolderRow, SkillProjectRow};
use crate::store::persistence::lessons::skill_review_detail::{
    SkillDraftDetail, SkillDraftLesson, SkillDraftRevision,
};

#[derive(Debug, Serialize)]
struct ProjectsResponse {
    projects: Vec<SkillProjectRow>,
    waiting: i64,
}

pub(crate) async fn list_projects(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.list_skill_projects().await {
        Ok(projects) => {
            let waiting = projects.iter().map(|p| p.waiting).sum();
            Json(ProjectsResponse { projects, waiting }).into_response()
        }
        Err(e) => persist_failed(&e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct FolderParams {
    project: Uuid,
}

#[derive(Debug, Serialize)]
struct FolderView {
    #[serde(flatten)]
    row: SkillFolderRow,
    state: SkillState,
    needs_you: bool,
}

#[derive(Debug, Serialize)]
struct FoldersResponse {
    project: Uuid,
    mode: String,
    mode_set: bool,
    skills: Vec<FolderView>,
}

pub(crate) async fn list_folders(
    State(state): State<AppState>,
    Query(params): Query<FolderParams>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (mode_code, mode_set) = match persist.skill_project_mode(params.project).await {
        Ok(Some(found)) => found,
        Ok(None) => return error(StatusCode::NOT_FOUND, "no such project"),
        Err(e) => return persist_failed(&e),
    };
    let mode = Mode::parse(&mode_code).unwrap_or(Mode::Ask);
    match persist.list_project_skill_folders(params.project).await {
        Ok(rows) => Json(FoldersResponse {
            project: params.project,
            mode: mode_code,
            mode_set,
            skills: rows
                .into_iter()
                .map(|row| FolderView {
                    state: skill_state(&row, mode),
                    needs_you: needs_you(&row, mode),
                    row,
                })
                .collect(),
        })
        .into_response(),
        Err(e) => persist_failed(&e),
    }
}

#[derive(Debug, Serialize)]
struct DraftResponse {
    draft: SkillDraftDetail,
    lessons: Vec<SkillDraftLesson>,
    revisions: Vec<SkillDraftRevision>,
    folder: Option<FolderView>,
    mode: String,
}

async fn folder_view(
    persist: &PersistHandle,
    draft: &SkillDraftDetail,
    mode: Mode,
) -> Result<Option<FolderView>, Response> {
    let rows = persist
        .list_project_skill_folders(draft.project_uuid)
        .await
        .map_err(|e| persist_failed(&e))?;
    Ok(rows
        .into_iter()
        .find(|row| row.folder == draft.folder)
        .map(|row| FolderView {
            state: skill_state(&row, mode),
            needs_you: needs_you(&row, mode),
            row,
        }))
}

pub(crate) async fn get_draft(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let draft = match persist.skill_draft_detail(id).await {
        Ok(Some(draft)) => draft,
        Ok(None) => return error(StatusCode::NOT_FOUND, "no such skill draft"),
        Err(e) => return persist_failed(&e),
    };
    let mode_code = match persist.skill_project_mode(draft.project_uuid).await {
        Ok(found) => found.map_or_else(|| "ask".to_owned(), |(code, _)| code),
        Err(e) => return persist_failed(&e),
    };
    let mode = Mode::parse(&mode_code).unwrap_or(Mode::Ask);
    let lessons = match persist.skill_draft_source_lessons(&draft.lesson_ids).await {
        Ok(rows) => rows,
        Err(e) => return persist_failed(&e),
    };
    let revisions = match persist
        .skill_draft_revisions(draft.project_id, &draft.folder)
        .await
    {
        Ok(rows) => rows,
        Err(e) => return persist_failed(&e),
    };
    let folder = match folder_view(persist, &draft, mode).await {
        Ok(view) => view,
        Err(resp) => return resp,
    };
    Json(DraftResponse {
        draft,
        lessons,
        revisions,
        folder,
        mode: mode_code,
    })
    .into_response()
}
