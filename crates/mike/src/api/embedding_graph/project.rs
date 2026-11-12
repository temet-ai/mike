use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::GraphEdgeDto;
use super::bad_request;
use crate::config::AppState;
use crate::web_base::persist_error_response;
use crate::web_base::persist_unavailable_response;

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectEmbeddingGraphParams {
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) min_similarity: Option<f64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectCentroidDto {
    pub(crate) present: bool,
    pub(crate) session_count: Option<i32>,
    pub(crate) embedded_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectGraphNodeDto {
    pub(crate) session_id: String,
    pub(crate) title: Option<String>,
    pub(crate) outcome: Option<String>,
    pub(crate) last_seen: String,
    pub(crate) last_status: String,
    pub(crate) similarity_to_centroid: Option<f64>,
    pub(crate) source: String,
    pub(crate) topics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SiblingProjectDto {
    pub(crate) project_id: i64,
    pub(crate) project_path: String,
    pub(crate) similarity: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectEmbeddingGraphResponse {
    pub(crate) project_id: i64,
    pub(crate) project_path: String,
    pub(crate) centroid: ProjectCentroidDto,
    pub(crate) nodes: Vec<ProjectGraphNodeDto>,
    pub(crate) edges: Vec<GraphEdgeDto>,
    pub(crate) sibling_projects: Vec<SiblingProjectDto>,
}

pub(crate) async fn get_project_embedding_graph(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<ProjectEmbeddingGraphParams>,
) -> Result<Json<ProjectEmbeddingGraphResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let limit = params.limit.unwrap_or(60);
    if !(1..=500).contains(&limit) {
        return Err(bad_request("`limit` must be between 1 and 500"));
    }
    let min_similarity = params.min_similarity.unwrap_or(0.6);
    if !(0.0..=1.0).contains(&min_similarity) {
        return Err(bad_request("`min_similarity` must be between 0.0 and 1.0"));
    }

    let Some(project_path) = handle.fetch_project_path(id).await.map_err(|e| {
        tracing::warn!(project_id = id, error = %e, "get_project_embedding_graph: fetch_project_path failed");
        persist_error_response(e)
    })?
    else {
        return Err((StatusCode::NOT_FOUND, "unknown project").into_response());
    };

    let centroid_status = handle.project_centroid_status(id).await.map_err(|e| {
        tracing::warn!(project_id = id, error = %e, "get_project_embedding_graph: centroid status fetch failed");
        persist_error_response(e)
    })?;
    let centroid_dto = ProjectCentroidDto {
        present: centroid_status.present,
        session_count: centroid_status.session_count,
        embedded_at: centroid_status.embedded_at.map(|t| t.to_rfc3339()),
    };

    if !centroid_status.present {
        return Ok(Json(ProjectEmbeddingGraphResponse {
            project_id: id,
            project_path,
            centroid: centroid_dto,
            nodes: Vec::new(),
            edges: Vec::new(),
            sibling_projects: Vec::new(),
        }));
    }

    const PER_NODE_K: i64 = 3;
    let (node_rows, edge_rows) = handle
        .project_session_graph(id, limit, min_similarity, PER_NODE_K)
        .await
        .map_err(|e| {
            tracing::warn!(project_id = id, error = %e, "get_project_embedding_graph: project_session_graph failed");
            persist_error_response(e)
        })?;
    let nodes = node_rows
        .into_iter()
        .map(|n| ProjectGraphNodeDto {
            session_id: n.session_id.to_string(),
            title: n.title,
            outcome: n.outcome,
            last_seen: n.last_seen.to_rfc3339(),
            last_status: n.last_status.unwrap_or_default(),
            similarity_to_centroid: n.similarity_to_centroid,
            source: n.source,
            topics: n.topics,
        })
        .collect();
    let edges = edge_rows
        .into_iter()
        .map(|e| GraphEdgeDto {
            from: e.from_session_id.to_string(),
            to: e.to_session_id.to_string(),
            similarity: e.similarity,
        })
        .collect();

    const SIBLING_K: i64 = 5;
    let siblings = handle.sibling_projects(id, SIBLING_K).await.map_err(|e| {
        tracing::warn!(project_id = id, error = %e, "get_project_embedding_graph: sibling_projects failed");
        persist_error_response(e)
    })?;
    let sibling_projects = siblings
        .into_iter()
        .map(|s| SiblingProjectDto {
            project_id: s.project_id,
            project_path: s.project_path,
            similarity: s.similarity,
        })
        .collect();

    Ok(Json(ProjectEmbeddingGraphResponse {
        project_id: id,
        project_path,
        centroid: centroid_dto,
        nodes,
        edges,
        sibling_projects,
    }))
}
