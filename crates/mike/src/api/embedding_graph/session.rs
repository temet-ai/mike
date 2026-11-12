use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::GraphEdgeDto;
use super::bad_request;
use crate::config::AppState;
use crate::web_base::{persist_error_response, persist_unavailable_response};

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum EmbeddingStatusDto {
    Present {
        present: bool,
        source: String,
        model: String,
        embedded_at: String,
        chunk_count: i32,
    },
    Absent {
        present: bool,
        reason: &'static str,
    },
}

#[derive(Debug, Deserialize)]
pub(crate) struct SessionEmbeddingGraphParams {
    #[serde(default)]
    pub(crate) k: Option<i64>,
    #[serde(default)]
    pub(crate) scope: Option<String>,
    #[serde(default)]
    pub(crate) min_similarity: Option<f64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionGraphNodeDto {
    pub(crate) session_id: String,
    pub(crate) title: Option<String>,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: String,
    pub(crate) outcome: Option<String>,
    pub(crate) last_seen: String,
    pub(crate) last_status: String,
    pub(crate) similarity: f64,
    pub(crate) source: String,
    pub(crate) topics: Vec<String>,
    pub(crate) is_centre: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionEmbeddingGraphResponse {
    pub(crate) session_id: String,
    pub(crate) embedding: EmbeddingStatusDto,
    pub(crate) nodes: Vec<SessionGraphNodeDto>,
    pub(crate) edges: Vec<GraphEdgeDto>,
}

enum SessionGraphParamError {
    KOutOfRange,
    InvalidScope,
    MinSimilarityOutOfRange,
}

fn session_graph_param_error_response(error: SessionGraphParamError) -> Response {
    match error {
        SessionGraphParamError::KOutOfRange => bad_request("`k` must be between 1 and 32"),
        SessionGraphParamError::InvalidScope => {
            bad_request("`scope` must be \"project\" or \"all\"")
        }
        SessionGraphParamError::MinSimilarityOutOfRange => {
            bad_request("`min_similarity` must be between 0.0 and 1.0")
        }
    }
}

fn validate_session_graph_params(
    params: &SessionEmbeddingGraphParams,
) -> Result<(i64, bool, f64), SessionGraphParamError> {
    let k = params.k.unwrap_or(8);
    if !(1..=32).contains(&k) {
        return Err(SessionGraphParamError::KOutOfRange);
    }
    let scope = params.scope.as_deref().unwrap_or("project");
    let same_project = match scope {
        "project" => true,
        "all" => false,
        _ => return Err(SessionGraphParamError::InvalidScope),
    };
    let min_similarity = params.min_similarity.unwrap_or(0.6);
    if !(0.0..=1.0).contains(&min_similarity) {
        return Err(SessionGraphParamError::MinSimilarityOutOfRange);
    }
    Ok((k, same_project, min_similarity))
}

async fn resolve_embedding_status(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: Uuid,
    id: &str,
    embedding_available: bool,
) -> Result<(EmbeddingStatusDto, String, bool), Response> {
    let status = handle.session_embedding_status(session_uuid).await.map_err(|e| {
        tracing::warn!(session_id = %id, error = %e, "get_session_embedding_graph: embedding status fetch failed");
        persist_error_response(e)
    })?;

    let dto = match &status {
        Some(s) => EmbeddingStatusDto::Present {
            present: true,
            source: s.source.clone(),
            model: s.model.clone(),
            embedded_at: s.embedded_at.to_rfc3339(),
            chunk_count: s.chunk_count,
        },
        None => {
            let reason = if embedding_available {
                "no_embedded_turns"
            } else {
                "embedding_disabled"
            };
            EmbeddingStatusDto::Absent {
                present: false,
                reason,
            }
        }
    };
    let source = status
        .as_ref()
        .map(|s| s.source.clone())
        .unwrap_or_else(|| "none".to_string());
    let has_vector = status.is_some();
    Ok((dto, source, has_vector))
}

async fn fetch_centre_topics(
    handle: &crate::store::persistence::PersistHandle,
    session_uuid: Uuid,
    id: &str,
) -> Vec<String> {
    match handle.fetch_session_knowledge(session_uuid).await {
        Ok(Some(k)) => k.topics,
        Ok(None) => Vec::new(),
        Err(e) => {
            tracing::warn!(session_id = %id, error = %e, "get_session_embedding_graph: knowledge fetch failed");
            Vec::new()
        }
    }
}

struct NeighbourGraphRequest<'a> {
    id: &'a str,
    session_uuid: Uuid,
    k: i64,
    same_project: bool,
    min_similarity: f64,
    centre_node: SessionGraphNodeDto,
}

async fn build_neighbour_graph(
    handle: &crate::store::persistence::PersistHandle,
    req: NeighbourGraphRequest<'_>,
) -> Result<(Vec<SessionGraphNodeDto>, Vec<GraphEdgeDto>), Response> {
    let NeighbourGraphRequest {
        id,
        session_uuid,
        k,
        same_project,
        min_similarity,
        centre_node,
    } = req;

    let neighbours = handle
        .nearest_sessions(session_uuid, k, same_project, min_similarity)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %id, error = %e, "get_session_embedding_graph: nearest_sessions failed");
            persist_error_response(e)
        })?;

    let mut nodes = Vec::with_capacity(neighbours.len() + 1);
    nodes.push(centre_node);
    let mut edges = Vec::with_capacity(neighbours.len());
    for n in &neighbours {
        edges.push(GraphEdgeDto {
            from: id.to_string(),
            to: n.session_id.to_string(),
            similarity: n.similarity,
        });
        nodes.push(SessionGraphNodeDto {
            session_id: n.session_id.to_string(),
            title: n.title.clone(),
            project_id: Some(n.project_id),
            project_path: n.project_path.clone(),
            outcome: n.outcome.clone(),
            last_seen: n.last_seen.to_rfc3339(),
            last_status: n.last_status.clone().unwrap_or_default(),
            similarity: n.similarity,
            source: n.source.clone(),
            topics: n.topics.clone(),
            is_centre: false,
        });
    }

    if neighbours.len() > 1 {
        let neighbour_ids: Vec<Uuid> = neighbours.iter().map(|n| n.session_id).collect();
        let pairwise = handle
            .pairwise_session_similarities(&neighbour_ids)
            .await
            .map_err(|e| {
                tracing::warn!(session_id = %id, error = %e, "get_session_embedding_graph: pairwise similarities failed");
                persist_error_response(e)
            })?;
        for (a, b, similarity) in pairwise {
            if similarity >= min_similarity {
                edges.push(GraphEdgeDto {
                    from: a.to_string(),
                    to: b.to_string(),
                    similarity,
                });
            }
        }
    }

    Ok((nodes, edges))
}

pub(crate) async fn get_session_embedding_graph(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<SessionEmbeddingGraphParams>,
) -> Result<Json<SessionEmbeddingGraphResponse>, Response> {
    let Ok(session_uuid) = Uuid::parse_str(&id) else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let (k, same_project, min_similarity) =
        validate_session_graph_params(&params).map_err(session_graph_param_error_response)?;

    let session_db_id = handle.resolve_session_db_id(&id).await.map_err(|e| {
        tracing::warn!(session_id = %id, error = %e, "get_session_embedding_graph: session resolve failed");
        persist_error_response(e)
    })?;
    let Some(session_db_id) = session_db_id else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let Some(summary) = handle.fetch_session_summary(session_db_id).await.map_err(|e| {
        tracing::warn!(session_id = %id, error = %e, "get_session_embedding_graph: session summary fetch failed");
        persist_error_response(e)
    })?
    else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };

    let (embedding_dto, centre_source, has_vector) = resolve_embedding_status(
        handle,
        session_uuid,
        &id,
        state.embedding_service().is_available(),
    )
    .await?;

    let centre_topics = fetch_centre_topics(handle, session_uuid, &id).await;
    let centre_node = SessionGraphNodeDto {
        session_id: id.clone(),
        title: summary.title,
        project_id: summary.project_id,
        project_path: summary.project_path,
        outcome: summary.session_outcome,
        last_seen: summary.last_seen.to_rfc3339(),
        last_status: summary.last_status.unwrap_or_default(),
        similarity: 1.0,
        source: centre_source,
        topics: centre_topics,
        is_centre: true,
    };

    if !has_vector {
        return Ok(Json(SessionEmbeddingGraphResponse {
            session_id: id,
            embedding: embedding_dto,
            nodes: vec![centre_node],
            edges: Vec::new(),
        }));
    }

    let (nodes, edges) = build_neighbour_graph(
        handle,
        NeighbourGraphRequest {
            id: &id,
            session_uuid,
            k,
            same_project,
            min_similarity,
            centre_node,
        },
    )
    .await?;

    Ok(Json(SessionEmbeddingGraphResponse {
        session_id: id,
        embedding: embedding_dto,
        nodes,
        edges,
    }))
}
