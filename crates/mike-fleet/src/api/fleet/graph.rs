use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::base_state::BaseState;
use crate::fleet::derive::{
    derive_error_kind, derive_quality, derive_role, derive_state, derive_state_detail,
};
use crate::store::persistence::PersistHandle;
use crate::web_base::persist_error_response;
use crate::workspace::Liveness;

#[derive(Debug, Deserialize)]
pub struct FleetGraphQuery {
    project: String,
    #[serde(default)]
    window_hours: Option<i32>,
}

#[derive(Serialize)]
pub struct FleetGraphNodeDto {
    pub session_id: String,
    pub title: Option<String>,
    pub agent_type: Option<String>,
    pub role: Option<String>,
    pub model: Option<String>,
    pub workspace: Option<String>,
    pub cwd: Option<String>,
    pub worktree: Option<String>,
    pub goal: Option<String>,
    pub next_steps: Option<String>,
    pub state: &'static str,
    pub state_detail: Option<String>,
    pub error_kind: Option<&'static str>,
    pub quality: Option<FleetQualityDto>,
    pub last_activity_at: String,
    pub request_count: i64,
    pub unconsumed_mail: i64,
}

#[derive(Serialize)]
pub struct FleetQualityDto {
    pub outcome: String,
    pub rationale: String,
}

#[derive(Serialize)]
pub struct FleetGraphEdgeDto {
    pub from_session_id: String,
    pub to_session_id: String,
    pub kind: &'static str,
    pub first_ts: String,
    pub last_ts: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct FleetGraphDto {
    pub project_path: String,
    pub generated_at: String,
    pub nodes: Vec<FleetGraphNodeDto>,
    pub edges: Vec<FleetGraphEdgeDto>,
}

fn edge_dto(
    edge: crate::store::persistence::fleet_correlation::FleetEdgeRow,
    kind: &'static str,
    id_to_uuid: &HashMap<i64, Uuid>,
) -> FleetGraphEdgeDto {
    FleetGraphEdgeDto {
        from_session_id: id_to_uuid
            .get(&edge.from_session_id)
            .map(Uuid::to_string)
            .unwrap_or_default(),
        to_session_id: id_to_uuid
            .get(&edge.to_session_id)
            .map(Uuid::to_string)
            .unwrap_or_default(),
        kind,
        first_ts: edge.first_ts.to_rfc3339(),
        last_ts: edge.last_ts.to_rfc3339(),
        count: edge.count,
    }
}

async fn resolve_fleet_graph_project(
    handle: &PersistHandle,
    project_path: &str,
) -> Result<i64, Response> {
    match handle.find_project_id_by_path(project_path).await {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(super::fleet_error(
            StatusCode::NOT_FOUND,
            "project_not_found",
            format!("no project registered at path {project_path}"),
        )),
        Err(e) => Err(persist_error_response(e)),
    }
}

struct FleetNodeRowFields {
    session_uuid: Uuid,
    title: Option<String>,
    agent_type: Option<String>,
    declared_role: Option<String>,
    model: Option<String>,
    cwd: Option<String>,
    worktree: Option<String>,
    goal: Option<String>,
    next_steps: Option<String>,
    last_activity_at: DateTime<Utc>,
    registration_liveness: Option<Liveness>,
    last_status: Option<String>,
    session_outcome: Option<String>,
    outcome_rationale: Option<String>,
    request_count: i64,
    unconsumed_mail: i64,
}

fn build_fleet_graph_node(
    row: FleetNodeRowFields,
    busy_ids: &std::collections::HashSet<String>,
    now: DateTime<Utc>,
) -> FleetGraphNodeDto {
    let session_uuid_str = row.session_uuid.to_string();
    let busy = busy_ids.contains(&session_uuid_str);
    let state_label = derive_state(
        now,
        busy,
        Some(row.last_activity_at),
        row.registration_liveness,
        row.last_status.as_deref(),
    );
    let state_detail = derive_state_detail(state_label, row.last_status.as_deref());
    let error_kind = derive_error_kind(state_label, row.last_status.as_deref());
    let quality = derive_quality(
        row.session_outcome.as_deref(),
        row.outcome_rationale.as_deref(),
    )
    .map(|q| FleetQualityDto {
        outcome: q.outcome,
        rationale: q.rationale,
    });
    let role = derive_role(row.declared_role.as_deref());
    FleetGraphNodeDto {
        session_id: session_uuid_str,
        title: row.title,
        agent_type: row.agent_type,
        role,
        model: row.model,
        workspace: None,
        cwd: row.cwd,
        worktree: row.worktree,
        goal: row.goal,
        next_steps: row.next_steps,
        state: state_label,
        state_detail,
        error_kind,
        quality,
        last_activity_at: row.last_activity_at.to_rfc3339(),
        request_count: row.request_count,
        unconsumed_mail: row.unconsumed_mail,
    }
}

pub async fn get_fleet_graph(
    State(state): State<BaseState>,
    Query(params): Query<FleetGraphQuery>,
) -> Result<Json<FleetGraphDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(crate::web_base::persist_unavailable_response());
    };

    let project_id = resolve_fleet_graph_project(handle, &params.project).await?;

    let window_hours = params
        .window_hours
        .filter(|h| *h > 0)
        .unwrap_or(super::DEFAULT_WINDOW_HOURS);

    let node_rows = handle
        .fleet()
        .fetch_fleet_nodes(project_id, window_hours)
        .await
        .map_err(persist_error_response)?;
    let session_ids: Vec<i64> = node_rows.iter().map(|n| n.session_id).collect();
    let id_to_uuid: HashMap<i64, Uuid> = node_rows
        .iter()
        .map(|n| (n.session_id, n.session_uuid))
        .collect();

    let message_edges = handle
        .fleet()
        .fetch_fleet_message_edges(&session_ids)
        .await
        .map_err(persist_error_response)?;

    let busy_ids = state.in_flight.busy_session_ids();
    let now = Utc::now();
    let nodes = node_rows
        .into_iter()
        .map(|row| {
            let fields = FleetNodeRowFields {
                session_uuid: row.session_uuid,
                title: row.title,
                agent_type: row.agent_type,
                declared_role: row.declared_role,
                model: row.model,
                cwd: row.cwd,
                worktree: row.worktree,
                goal: row.goal,
                next_steps: row.next_steps,
                last_activity_at: row.last_activity_at,
                registration_liveness: row.registration_liveness,
                last_status: row.last_status,
                session_outcome: row.session_outcome,
                outcome_rationale: row.outcome_rationale,
                request_count: row.request_count,
                unconsumed_mail: row.unconsumed_mail,
            };
            build_fleet_graph_node(fields, &busy_ids, now)
        })
        .collect();

    let edges: Vec<FleetGraphEdgeDto> = message_edges
        .into_iter()
        .map(|e| edge_dto(e, "messaged", &id_to_uuid))
        .collect();

    Ok(Json(FleetGraphDto {
        project_path: params.project,
        generated_at: Utc::now().to_rfc3339(),
        nodes,
        edges,
    }))
}
