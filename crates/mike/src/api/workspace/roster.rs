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
pub(crate) struct RosterEntryDto {
    pub(crate) observation: Option<crate::store::persistence::runtime::SessionObservationRow>,
    pub(crate) session_id: Uuid,
    pub(crate) is_self: bool,
    pub(crate) cwd: String,
    pub(crate) worktree: Option<String>,
    pub(crate) registered_via: String,
    pub(crate) registered_at: String,
    pub(crate) lease_expires_at: String,
    pub(crate) released_at: Option<String>,
    pub(crate) liveness: &'static str,
    pub(crate) name: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) next_steps: Option<String>,
    pub(crate) busy: bool,
    pub(crate) last_request_at: Option<String>,
    pub(crate) newest_request_number: Option<i64>,
    pub(crate) channel_present: bool,
    pub(crate) channel_cse_id: Option<String>,
    pub(crate) heartbeat_age_seconds: Option<i64>,
    pub(crate) heartbeat_interval_seconds: Option<i32>,
    pub(crate) idle_seconds: Option<i32>,
    pub(crate) session_state: Option<String>,
    pub(crate) session_state_at: Option<String>,
    pub(crate) reply_channel_open: bool,
    pub(crate) reply_channel_opened_at: Option<String>,
    pub(crate) last_inbound_message_at: Option<String>,
    pub(crate) post_turn_summary: Option<serde_json::Value>,
    pub(crate) post_turn_summary_at: Option<String>,
    pub(crate) client_version: Option<String>,
    pub(crate) liveness_source: &'static str,
}

pub(super) fn roster_entry_from_row(
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
pub(crate) struct RosterResponseDto {
    pub(crate) project_id: i64,
    pub(crate) project_path: String,
    pub(crate) roster: Vec<RosterEntryDto>,
}

pub(crate) async fn resolve_project_path(
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
pub(crate) struct RosterQuery {
    #[serde(default)]
    pub(crate) include_inactive: bool,
}

pub(crate) async fn get_project_roster(
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
pub(crate) struct ActivityParams {
    #[serde(default)]
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CoordinationEventDto {
    pub(crate) id: i64,
    pub(crate) session_id: Option<Uuid>,
    pub(crate) actor_label: String,
    pub(crate) kind: String,
    pub(crate) subject: String,
    pub(crate) detail: serde_json::Value,
    pub(crate) created_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ActivityResponseDto {
    pub(crate) project_id: i64,
    pub(crate) project_path: String,
    pub(crate) total_count: i64,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) events: Vec<CoordinationEventDto>,
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

pub(crate) async fn get_project_activity(
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
