use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::api::handlers::{
    LessonDto, lesson_error, persist_error_response, persist_unavailable_response,
};
use crate::config::AppState;
use crate::store::persistence::consolidation_reads::{ArchivedLessonRow, RestoreLessonOutcome};

#[derive(Debug, Serialize)]
pub(crate) struct ArchivedLessonDto {
    pub(crate) id: i64,
    pub(crate) uuid: String,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) topic: Option<String>,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) source_turn_id: Option<i64>,
    pub(crate) agent_id: Option<i64>,
    pub(crate) created_at: String,
    pub(crate) archived_at: Option<String>,
    pub(crate) archived_reason: Option<String>,
    pub(crate) duplicate_of_id: Option<i64>,
}

impl From<ArchivedLessonRow> for ArchivedLessonDto {
    fn from(r: ArchivedLessonRow) -> Self {
        Self {
            id: r.id,
            uuid: r.uuid,
            project_id: r.project_id,
            project_path: r.project_path,
            kind: r.kind,
            text: r.text,
            topic: r.topic,
            session_id: r.session_id,
            session_uuid: r.session_uuid,
            source_turn_id: r.source_turn_id,
            agent_id: r.agent_id,
            created_at: r.created_at.to_rfc3339(),
            archived_at: r.archived_at.map(|t| t.to_rfc3339()),
            archived_reason: r.archived_reason,
            duplicate_of_id: r.duplicate_of_id,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ArchivedLessonsListParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
}

pub(crate) async fn list_archived_lessons(
    State(state): State<AppState>,
    Query(params): Query<ArchivedLessonsListParams>,
) -> Result<Json<Vec<ArchivedLessonDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Some(project) = params.project.filter(|p| !p.trim().is_empty()) else {
        return Err(lesson_error(
            StatusCode::BAD_REQUEST,
            "`project` is required",
        ));
    };
    let rows = handle.list_archived_lessons(&project).await.map_err(|e| {
        tracing::warn!(project = %project, error = %e, "list_archived_lessons: query failed");
        persist_error_response(e)
    })?;
    Ok(Json(
        rows.into_iter().map(ArchivedLessonDto::from).collect(),
    ))
}

pub(crate) async fn restore_lesson(
    State(state): State<AppState>,
    Path(lesson_id): Path<i64>,
) -> Result<Json<LessonDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let outcome = handle.restore_lesson(lesson_id).await.map_err(|e| {
        tracing::warn!(lesson_id, error = %e, "restore_lesson: query failed");
        persist_error_response(e)
    })?;
    match outcome {
        RestoreLessonOutcome::NotFound => {
            Err(lesson_error(StatusCode::NOT_FOUND, "lesson not found"))
        }
        RestoreLessonOutcome::NotArchived => {
            Err(lesson_error(StatusCode::CONFLICT, "lesson is not archived"))
        }
        RestoreLessonOutcome::Restored(row) => Ok(Json(LessonDto {
            id: row.id,
            project_id: row.project_id,
            project_path: row.project_path,
            kind: row.kind,
            text: row.text,
            source_turn_id: row.source_turn_id,
            created_at: row.created_at.to_rfc3339(),
            expires_at: row.expires_at.map(|t| t.to_rfc3339()),
            topic: row.topic,
        })),
    }
}
