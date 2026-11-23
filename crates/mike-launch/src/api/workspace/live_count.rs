use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use uuid::Uuid;

use crate::base_state::BaseState;
use crate::store::persistence::workspace::roster::LiveCount;
use crate::store::persistence::workspace::{PathMatch, ProjectPathResolution};
use crate::web_base::{persist_error_response, persist_unavailable_response};

#[derive(Debug, Deserialize, Default)]
pub struct LiveCountQuery {
    #[serde(default)]
    pub exclude: Option<String>,
}

fn excluded(raw: Option<&str>) -> Result<Option<Uuid>, Response> {
    raw.map(|raw| {
        Uuid::parse_str(raw).map_err(|error| {
            super::workspace_error(
                StatusCode::BAD_REQUEST,
                "invalid_exclude",
                format!("exclude `{raw}` is not a session uuid: {error}"),
            )
        })
    })
    .transpose()
}

pub async fn get_project_live_count(
    State(state): State<BaseState>,
    Path(project_path): Path<String>,
    Query(query): Query<LiveCountQuery>,
) -> Result<Json<LiveCount>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let exclude = excluded(query.exclude.as_deref())?;
    let resolution = handle
        .resolve_project(&project_path, PathMatch::OrSuffix)
        .await
        .map_err(persist_error_response)?;
    let project_id = match resolution {
        ProjectPathResolution::Resolved(id, _) => id,
        ProjectPathResolution::NotFound => return Ok(Json(LiveCount { live: 0 })),
        ProjectPathResolution::Ambiguous(candidates) => {
            return Err(super::workspace_error(
                StatusCode::CONFLICT,
                "ambiguous_project_path",
                format!(
                    "path suffix `{project_path}` matches {} projects: {} — pass more of the path to disambiguate",
                    candidates.len(),
                    candidates.join(", ")
                ),
            ));
        }
    };
    handle
        .count_live_peers(project_id, exclude)
        .await
        .map(Json)
        .map_err(persist_error_response)
}

#[cfg(test)]
#[path = "live_count_tests.rs"]
mod live_count_tests;
