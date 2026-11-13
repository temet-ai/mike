use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::{
    CoordinationEventRow, PathMatch, ProjectPathResolution, WorkspaceRosterRow,
};
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::workspace::RosterScope;

#[derive(Debug, Clone, Serialize)]
pub struct RosterEntryDto {
    pub observation: Option<crate::store::persistence::runtime::SessionObservationRow>,
    pub session_id: Uuid,
    pub is_self: bool,
    pub cwd: String,
    pub worktree: Option<String>,
    pub registered_via: String,
    pub registered_at: String,
    pub lease_expires_at: String,
    pub released_at: Option<String>,
    pub liveness: &'static str,
    pub name: Option<String>,
    pub role: Option<String>,
    pub instructions: Option<String>,
    pub goal: Option<String>,
    pub next_steps: Option<String>,
    pub busy: bool,
    pub last_request_at: Option<String>,
    pub newest_request_number: Option<i64>,
    pub channel_present: bool,
    pub channel_cse_id: Option<String>,
    pub heartbeat_age_seconds: Option<i64>,
    pub heartbeat_interval_seconds: Option<i32>,
    pub idle_seconds: Option<i32>,
    pub session_state: Option<String>,
    pub session_state_at: Option<String>,
    pub reply_channel_open: bool,
    pub reply_channel_opened_at: Option<String>,
    pub last_inbound_message_at: Option<String>,
    pub post_turn_summary: Option<serde_json::Value>,
    pub post_turn_summary_at: Option<String>,
    pub client_version: Option<String>,
    pub liveness_source: &'static str,
}

pub fn roster_entry_from_row(
    row: WorkspaceRosterRow,
    self_uuid: Option<Uuid>,
    busy_ids: &std::collections::HashSet<String>,
) -> RosterEntryDto {
    let busy = busy_ids.contains(&row.session_uuid.to_string());
    RosterEntryDto {
        observation: None,
        session_id: row.session_uuid,
        is_self: self_uuid == Some(row.session_uuid),
        cwd: row.cwd,
        worktree: row.worktree,
        registered_via: row.registered_via,
        registered_at: row.registered_at.to_rfc3339(),
        lease_expires_at: row.lease_expires_at.to_rfc3339(),
        released_at: row.released_at.map(|t| t.to_rfc3339()),
        liveness: row.liveness.as_str(),
        name: row.display_name,
        role: row.role,
        instructions: row.instructions,
        goal: row.goal,
        next_steps: row.next_steps,
        busy,
        last_request_at: row.last_request_at.map(|t| t.to_rfc3339()),
        newest_request_number: row.newest_request_number,
        channel_present: row.channel_present,
        channel_cse_id: row.channel_cse_id,
        heartbeat_age_seconds: row.heartbeat_age_seconds,
        heartbeat_interval_seconds: row.heartbeat_interval_seconds,
        idle_seconds: row.idle_seconds,
        session_state: row.session_state,
        session_state_at: row.session_state_at.map(|t| t.to_rfc3339()),
        reply_channel_open: row.reply_channel_open,
        reply_channel_opened_at: row.reply_channel_opened_at.map(|t| t.to_rfc3339()),
        last_inbound_message_at: row.last_inbound_message_at.map(|t| t.to_rfc3339()),
        post_turn_summary: row.post_turn_summary,
        post_turn_summary_at: row.post_turn_summary_at.map(|t| t.to_rfc3339()),
        client_version: row.client_version,
        liveness_source: row.liveness_source,
    }
}

#[derive(Debug, Serialize)]
pub struct RosterResponseDto {
    pub project_id: i64,
    pub project_path: String,
    pub roster: Vec<RosterEntryDto>,
}

pub async fn resolve_project_path(
    handle: &PersistHandle,
    project_path: String,
) -> Result<(i64, String), Response> {
    match handle
        .resolve_project(&project_path, PathMatch::OrSuffix)
        .await
    {
        Ok(ProjectPathResolution::Resolved(id, canonical)) => Ok((id, canonical)),
        Ok(ProjectPathResolution::Ambiguous(candidates)) => Err(super::workspace_error(
            StatusCode::CONFLICT,
            "ambiguous_project_path",
            format!(
                "path suffix `{project_path}` matches {} projects: {} — pass more of the path to disambiguate",
                candidates.len(),
                candidates.join(", ")
            ),
        )),
        Ok(ProjectPathResolution::NotFound) => {
            let known = handle
                .list_recent_project_paths(5)
                .await
                .unwrap_or_default();
            let hint = if known.is_empty() {
                String::new()
            } else {
                format!(" known project paths: {}", known.join(", "))
            };
            Err(super::workspace_error(
                StatusCode::NOT_FOUND,
                "project_not_found",
                format!("no project registered at path {project_path}.{hint}"),
            ))
        }
        Err(e) => Err(persist_error_response(e)),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct RosterQuery {
    #[serde(default)]
    pub include_inactive: bool,
}

pub async fn get_project_roster(
    State(state): State<AppState>,
    Path(project_path): Path<String>,
    Query(query): Query<RosterQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some(handle) = state.persist() else {
        return persist_unavailable_response();
    };
    let scope = if query.include_inactive {
        RosterScope::All
    } else {
        RosterScope::Live
    };
    let poll = crate::api::poll_answers::Poll {
        key: format!("roster:{}:{project_path}", query.include_inactive),
        freshness: crate::api::poll_answers::ROSTER,
        scope: Vec::new(),
        headers: &headers,
    };
    crate::api::poll_answers::answer(
        &state,
        state.poll_answers.rosters(),
        poll,
        || super::roster_poll::load_roster(handle, project_path, scope),
        |answer| super::roster_poll::present_roster(&state, answer),
    )
    .await
}

const ACTIVITY_DEFAULT_LIMIT: usize = 100;
const ACTIVITY_MAX_LIMIT: usize = 500;

#[derive(Debug, Deserialize, Default)]
pub struct ActivityParams {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct CoordinationEventDto {
    pub id: i64,
    pub session_id: Option<Uuid>,
    pub actor_label: String,
    pub kind: String,
    pub subject: String,
    pub detail: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ActivityResponseDto {
    pub project_id: i64,
    pub project_path: String,
    pub total_count: i64,
    pub offset: usize,
    pub limit: usize,
    pub events: Vec<CoordinationEventDto>,
}

fn coordination_event_dto(row: CoordinationEventRow) -> CoordinationEventDto {
    CoordinationEventDto {
        id: row.id,
        session_id: row.session_id,
        actor_label: row.actor_label,
        kind: row.kind,
        subject: row.subject,
        detail: row.detail,
        created_at: row.created_at.to_rfc3339(),
    }
}

pub async fn get_project_activity(
    State(state): State<AppState>,
    Path(project_path): Path<String>,
    Query(params): Query<ActivityParams>,
) -> Result<Json<ActivityResponseDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let (project_id, project_path) = resolve_project_path(handle, project_path).await?;

    let limit = params
        .limit
        .unwrap_or(ACTIVITY_DEFAULT_LIMIT)
        .clamp(1, ACTIVITY_MAX_LIMIT);
    let offset = params.offset.unwrap_or(0);

    let (total_count, rows) = handle
        .fetch_coordination_events(project_id, limit as i64, offset as i64)
        .await
        .map_err(persist_error_response)?;

    Ok(Json(ActivityResponseDto {
        project_id,
        project_path,
        total_count,
        offset,
        limit,
        events: rows.into_iter().map(coordination_event_dto).collect(),
    }))
}
