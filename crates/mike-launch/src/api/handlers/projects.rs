use crate::base_state::BaseState;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::listings::{ProjectDetailRow, ProjectSelector};
use crate::store::persistence::meta::pg_interval_to_humantime;
use crate::store::persistence::rehydrate::{ProjectCountsRow, ProjectListRow, ProjectLocationRow};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ProjectListEntry {
    pub project_id: i64,
    pub project_uuid: Uuid,
    pub project_path: String,
    pub first_seen: String,
    pub last_seen: String,
    pub pinned: bool,
    pub ttl_override: Option<String>,
    pub pin_created_at: Option<String>,
    pub ttl_override_set_at: Option<String>,
    pub custom_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectsQuery {
    #[serde(default)]
    counts: bool,
}

#[derive(Debug, Serialize)]
pub struct ProjectLocationDto {
    location_uuid: Uuid,
    machine_uuid: Uuid,
    machine_name: String,
    root_path: String,
    last_seen: String,
}

#[derive(Debug, Serialize)]
pub struct ProjectDto {
    #[serde(flatten)]
    project: ProjectListEntry,
    locations: Vec<ProjectLocationDto>,
}

impl From<ProjectListRow> for ProjectListEntry {
    fn from(r: ProjectListRow) -> Self {
        Self {
            project_id: r.project_id,
            project_uuid: r.project_uuid,
            project_path: r.project_path,
            first_seen: r.first_seen.to_rfc3339(),
            last_seen: r.last_seen.to_rfc3339(),
            pinned: r.pinned,
            ttl_override: r.ttl_override.and_then(pg_interval_to_humantime),
            pin_created_at: r.pin_created_at.map(|dt| dt.to_rfc3339()),
            ttl_override_set_at: r.ttl_override_set_at.map(|dt| dt.to_rfc3339()),
            custom_name: r.custom_name,
            session_count: None,
            request_count: None,
        }
    }
}

impl From<ProjectLocationRow> for ProjectLocationDto {
    fn from(r: ProjectLocationRow) -> Self {
        Self {
            location_uuid: r.location_uuid,
            machine_uuid: r.machine_uuid,
            machine_name: r.machine_name,
            root_path: r.root_path,
            last_seen: r.last_seen.to_rfc3339(),
        }
    }
}

impl From<ProjectDetailRow> for ProjectDto {
    fn from(r: ProjectDetailRow) -> Self {
        Self {
            project: ProjectListEntry {
                session_count: Some(r.project.session_count.max(0) as u64),
                request_count: Some(r.project.request_count.max(0) as u64),
                ..r.project.project.into()
            },
            locations: r.locations.into_iter().map(Into::into).collect(),
        }
    }
}

pub async fn list_projects(
    State(state): State<BaseState>,
    Query(query): Query<ListProjectsQuery>,
) -> Result<Json<Vec<ProjectListEntry>>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let rows = handle.launch().list_projects().await.map_err(|e| {
        tracing::warn!(error = %e, "list_projects: DB query failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut entries: Vec<ProjectListEntry> = rows.into_iter().map(Into::into).collect();
    if query.counts {
        let counts = handle.launch().list_project_counts().await.map_err(|e| {
            tracing::warn!(error = %e, "list_projects: counts query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let by_project: HashMap<i64, ProjectCountsRow> =
            counts.into_iter().map(|c| (c.project_id, c)).collect();
        for entry in &mut entries {
            let found = by_project.get(&entry.project_id);
            entry.session_count = Some(found.map_or(0, |c| c.session_count.max(0) as u64));
            entry.request_count = Some(found.map_or(0, |c| c.request_count.max(0) as u64));
        }
    }
    Ok(Json(entries))
}

pub async fn get_project(
    State(state): State<BaseState>,
    Path(project_uuid): Path<String>,
) -> Response {
    match Uuid::parse_str(&project_uuid) {
        Ok(uuid) => project_response(&state, ProjectSelector::Uuid(uuid), &project_uuid).await,
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "project_uuid `{project_uuid}` is not a UUID ({e}); \
                     use GET /api/projects/by-path/{{project_path}} to look a project up by path"
                ),
                "project_uuid": project_uuid,
            })),
        )
            .into_response(),
    }
}

pub async fn get_project_by_path(
    State(state): State<BaseState>,
    Path(project_path): Path<String>,
) -> Response {
    project_response(&state, ProjectSelector::Path(&project_path), &project_path).await
}

async fn project_response(
    state: &BaseState,
    selector: ProjectSelector<'_>,
    requested: &str,
) -> Response {
    let Some(handle) = state.persist() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "session persistence is disabled"})),
        )
            .into_response();
    };
    match handle.launch().fetch_project_detail(selector).await {
        Ok(Some(detail)) => Json(ProjectDto::from(detail)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("no project matches `{requested}`; list projects with GET /api/projects"),
                "requested": requested,
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(error = %e, requested, "get_project: DB query failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "project lookup failed"})),
            )
                .into_response()
        }
    }
}
