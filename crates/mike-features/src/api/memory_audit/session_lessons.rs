use crate::store::persistence::lessons_repo::LessonsPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;

use crate::api::handlers::lesson_error;
use crate::state::AppState;
use crate::store::persistence::lesson_injections::{
    SessionInjectedLessonRow, SessionRecordedLessonRow,
};
use crate::web_base::{persist_error_response, persist_unavailable_response};

#[derive(Debug, Serialize)]
pub struct SessionInjectedLessonDto {
    pub lesson_id: i64,
    pub lesson_text: String,
    pub kind: String,
    pub injection_count: i64,
    pub first_injected_at: String,
    pub last_injected_at: String,
    pub origins: Vec<String>,
}

impl From<SessionInjectedLessonRow> for SessionInjectedLessonDto {
    fn from(r: SessionInjectedLessonRow) -> Self {
        Self {
            lesson_id: r.lesson_id,
            lesson_text: r.lesson_text,
            kind: r.kind,
            injection_count: r.injection_count,
            first_injected_at: r.first_injected_at.to_rfc3339(),
            last_injected_at: r.last_injected_at.to_rfc3339(),
            origins: r.origins,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SessionRecordedLessonDto {
    pub lesson_id: i64,
    pub lesson_text: String,
    pub kind: String,
    pub created_at: String,
}

impl From<SessionRecordedLessonRow> for SessionRecordedLessonDto {
    fn from(r: SessionRecordedLessonRow) -> Self {
        Self {
            lesson_id: r.lesson_id,
            lesson_text: r.lesson_text,
            kind: r.kind,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SessionLessonsDto {
    pub injected: Vec<SessionInjectedLessonDto>,
    pub injected_count: i64,
    pub recorded: Vec<SessionRecordedLessonDto>,
    pub recorded_count: i64,
}

pub async fn list_session_lessons(
    State(state): State<AppState>,
    Path(session_uuid): Path<String>,
) -> Result<Json<SessionLessonsDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Ok(uuid) = uuid::Uuid::parse_str(&session_uuid) else {
        return Err(lesson_error(StatusCode::NOT_FOUND, "unknown session"));
    };
    let rows = handle.lessons().session_lessons(uuid).await.map_err(|e| {
        tracing::warn!(session_id = %session_uuid, error = %e, "list_session_lessons: query failed");
        persist_error_response(e)
    })?;
    let Some(rows) = rows else {
        return Err(lesson_error(StatusCode::NOT_FOUND, "unknown session"));
    };

    let injected: Vec<SessionInjectedLessonDto> = rows
        .injected
        .into_iter()
        .map(SessionInjectedLessonDto::from)
        .collect();
    let recorded: Vec<SessionRecordedLessonDto> = rows
        .recorded
        .into_iter()
        .map(SessionRecordedLessonDto::from)
        .collect();
    Ok(Json(SessionLessonsDto {
        injected_count: injected.len() as i64,
        recorded_count: recorded.len() as i64,
        injected,
        recorded,
    }))
}
