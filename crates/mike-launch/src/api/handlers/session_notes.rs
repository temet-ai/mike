use crate::api::changes::{WithTail, note_answer_tail};
use crate::base_state::BaseState;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::web_base::{persist_error_response, persist_unavailable_response};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

const NOTES_DEFAULT_LIMIT: usize = 100;
const NOTES_MAX_LIMIT: usize = 500;

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn parse_note_order(
    value: Option<&str>,
) -> Result<crate::store::persistence::session_notes::NoteOrder, Box<Response>> {
    use crate::store::persistence::session_notes::NoteOrder;
    match non_empty(value) {
        None | Some("desc") => Ok(NoteOrder::NewestFirst),
        Some("asc") => Ok(NoteOrder::OldestFirst),
        Some(other) => Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_order",
                    "message": format!("order must be `asc` or `desc`, got `{other}`"),
                })),
            )
                .into_response(),
        )),
    }
}

#[derive(Debug, Serialize)]
pub struct SessionNoteDto {
    pub note_uuid: uuid::Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

fn session_note_dto(
    row: crate::store::persistence::session_notes::SessionNoteRow,
) -> SessionNoteDto {
    SessionNoteDto {
        note_uuid: row.note_uuid,
        kind: row.kind,
        title: row.title,
        body: row.body,
        status: row.status,
        tags: row.tags,
        data: row.data,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectSessionNoteDto {
    pub session_uuid: uuid::Uuid,
    pub session_name: Option<String>,
    pub session_role: Option<String>,
    #[serde(flatten)]
    pub note: SessionNoteDto,
}

fn project_session_note_dto(
    row: crate::store::persistence::session_notes::SessionNoteWithAuthor,
) -> ProjectSessionNoteDto {
    ProjectSessionNoteDto {
        session_uuid: row.session_uuid,
        session_name: row.display_name,
        session_role: row.role,
        note: SessionNoteDto {
            note_uuid: row.note_uuid,
            kind: row.kind,
            title: row.title,
            body: row.body,
            status: row.status,
            tags: row.tags,
            data: row.data,
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
        },
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionNoteBody {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

pub async fn post_session_note(
    State(state): State<BaseState>,
    Path(session_uuid): Path<uuid::Uuid>,
    Json(body): Json<CreateSessionNoteBody>,
) -> Result<Json<WithTail<SessionNoteDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Some(title) = non_empty(body.title.as_deref()) else {
        return Err((
            StatusCode::BAD_REQUEST,
            "title is required when creating a note (note_uuid absent)",
        )
            .into_response());
    };
    let new_note = crate::store::persistence::session_notes::NewSessionNote {
        kind: non_empty(body.kind.as_deref()),
        title,
        body: non_empty(body.body.as_deref()),
        status: non_empty(body.status.as_deref()),
        tags: body.tags.as_deref(),
        data: body.data.as_ref(),
    };
    match handle
        .launch()
        .create_session_note(session_uuid, new_note)
        .await
    {
        Ok(row) => {
            record_note_in_change_record(&state, handle, session_uuid, &row);
            Ok(Json(with_tail(&state, session_uuid, row).await))
        }
        Err(crate::store::persistence::PersistError::InvalidAcceptanceObligation(issue)) => {
            Err((StatusCode::BAD_REQUEST, issue).into_response())
        }
        Err(crate::store::persistence::PersistError::DataInvariant(_)) => {
            Err((StatusCode::NOT_FOUND, "unknown session").into_response())
        }
        Err(e) => Err(persist_error_response(e)),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct SessionNotesQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

pub async fn get_session_notes(
    State(state): State<BaseState>,
    Path(session_uuid): Path<uuid::Uuid>,
    Query(query): Query<SessionNotesQuery>,
) -> Result<Json<Vec<SessionNoteDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let limit = query
        .limit
        .unwrap_or(NOTES_DEFAULT_LIMIT)
        .clamp(1, NOTES_MAX_LIMIT);
    let filter = crate::store::persistence::session_notes::SessionNoteFilter {
        kind: non_empty(query.kind.as_deref()),
        status: non_empty(query.status.as_deref()),
        tag: non_empty(query.tag.as_deref()),
        q: non_empty(query.q.as_deref()),
        order: parse_note_order(query.order.as_deref()).map_err(|e| *e)?,
        limit: limit as i64,
        offset: query.offset.unwrap_or(0) as i64,
    };
    let rows = handle
        .launch()
        .fetch_session_notes(session_uuid, filter)
        .await
        .map_err(persist_error_response)?;
    let notes: Vec<_> = rows.into_iter().map(session_note_dto).collect();
    Ok(Json(notes))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSessionNoteBody {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

pub async fn patch_session_note(
    State(state): State<BaseState>,
    Path((session_uuid, note_uuid)): Path<(uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<UpdateSessionNoteBody>,
) -> Result<Json<WithTail<SessionNoteDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    require_note_owner(handle, session_uuid, note_uuid).await?;
    let update = crate::store::persistence::session_notes::SessionNoteUpdate {
        kind: non_empty(body.kind.as_deref()),
        title: non_empty(body.title.as_deref()),
        body: body.body.as_deref(),
        status: non_empty(body.status.as_deref()),
        tags: body.tags.as_deref(),
        data: body.data.as_ref(),
    };
    match handle.launch().update_session_note(note_uuid, update).await {
        Ok(Some(row)) => {
            record_note_in_change_record(&state, handle, session_uuid, &row);
            Ok(Json(with_tail(&state, session_uuid, row).await))
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "no note with that id").into_response()),
        Err(crate::store::persistence::PersistError::InvalidAcceptanceObligation(issue)) => {
            Err((StatusCode::BAD_REQUEST, issue).into_response())
        }
        Err(e) => Err(persist_error_response(e)),
    }
}

async fn with_tail(
    state: &BaseState,
    session_uuid: uuid::Uuid,
    row: crate::store::persistence::session_notes::SessionNoteRow,
) -> WithTail<SessionNoteDto> {
    WithTail {
        report: session_note_dto(row),
        since_you_last_looked: note_answer_tail(state, session_uuid).await,
    }
}

fn record_note_in_change_record(
    state: &BaseState,
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: uuid::Uuid,
    row: &crate::store::persistence::session_notes::SessionNoteRow,
) {
    let enabled = state.feature_toggle_overlay.load().change_record;
    crate::change_record::writers::spawn_note_event(
        &state.tracker(),
        handle.clone(),
        enabled,
        session_uuid,
        crate::change_record::writers::NoteFacts {
            note_uuid: row.note_uuid,
            note_kind: row.kind.clone(),
            title: row.title.clone(),
            body: row.body.clone(),
            status: row.status.clone(),
            data: row.data.clone(),
            at: row.updated_at,
        },
    );
}

pub async fn delete_session_note(
    State(state): State<BaseState>,
    Path((session_uuid, note_uuid)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<StatusCode, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    require_note_owner(handle, session_uuid, note_uuid).await?;
    match handle.launch().delete_session_note(note_uuid).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, "no note with that id").into_response()),
        Err(crate::store::persistence::PersistError::InvalidAcceptanceObligation(issue)) => {
            Err((StatusCode::BAD_REQUEST, issue).into_response())
        }
        Err(e) => Err(persist_error_response(e)),
    }
}

async fn require_note_owner(
    handle: &crate::store::persistence::PersistHandle,
    session: uuid::Uuid,
    note: uuid::Uuid,
) -> Result<(), Response> {
    match handle.launch().session_note_belongs_to(session, note).await {
        Ok(true) => Ok(()),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            "no note with that id owned by this session",
        )
            .into_response()),
        Err(err) => Err(persist_error_response(err)),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ProjectNotesQuery {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub session_uuid: Option<uuid::Uuid>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ProjectNotesResponse {
    pub project_id: i64,
    pub project_path: String,
    pub total_count: i64,
    pub offset: usize,
    pub limit: usize,
    pub notes: Vec<ProjectSessionNoteDto>,
}

fn project_note_filter(
    query: &ProjectNotesQuery,
    limit: usize,
    offset: usize,
    order: crate::store::persistence::session_notes::NoteOrder,
) -> crate::store::persistence::session_notes::ProjectNoteFilter<'_> {
    crate::store::persistence::session_notes::ProjectNoteFilter {
        role: non_empty(query.role.as_deref()),
        name: non_empty(query.name.as_deref()),
        kind: non_empty(query.kind.as_deref()),
        status: non_empty(query.status.as_deref()),
        tag: non_empty(query.tag.as_deref()),
        session_uuid: query.session_uuid,
        q: non_empty(query.q.as_deref()),
        order,
        limit: limit as i64,
        offset: offset as i64,
    }
}

pub async fn get_project_notes(
    State(state): State<BaseState>,
    Path(project_path): Path<String>,
    Query(query): Query<ProjectNotesQuery>,
) -> Result<Json<ProjectNotesResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let (project_id, project_path) =
        crate::api::workspace::resolve_project_path(handle, project_path).await?;

    let limit = query
        .limit
        .unwrap_or(NOTES_DEFAULT_LIMIT)
        .clamp(1, NOTES_MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let order = parse_note_order(query.order.as_deref()).map_err(|e| *e)?;
    let filter = project_note_filter(&query, limit, offset, order);

    let (total_count, rows) = handle
        .launch()
        .fetch_project_notes(project_id, filter)
        .await
        .map_err(persist_error_response)?;

    let notes: Vec<_> = rows.into_iter().map(project_session_note_dto).collect();
    Ok(Json(ProjectNotesResponse {
        project_id,
        project_path,
        total_count,
        offset,
        limit,
        notes,
    }))
}
