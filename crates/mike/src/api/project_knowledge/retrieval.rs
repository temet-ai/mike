use super::knowledge_error;
use super::passage_excerpt::Passages;
use super::query_vector_cache::QUERY_VECTORS;
use super::rerank_page::QueryVector;
use crate::analysis::EmbeddingService;
use crate::api::rerank_search::RerankReport;
use crate::config::AppState;
use crate::embed_worker::knowledge::{model_key, valid_vector};
use crate::store::persistence::project_knowledge::{KnowledgeQuery, ProjectIdentity};
use crate::store::persistence::{PersistError, PersistHandle};
use axum::response::Response;
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) async fn query_vector(
    service: &EmbeddingService,
    key: &str,
    query: &str,
) -> Option<(String, String, String)> {
    let vector =
        match QUERY_VECTORS.get(key, query) {
            Some(cached) => cached,
            None => {
                let embedded =
                    match tokio::time::timeout(Duration::from_secs(3), service.embed_query(query))
                        .await
                    {
                        Ok(Ok(vector)) => vector,
                        _ => return None,
                    };
                if !valid_vector(&embedded, service.dim()) || model_key(service) != key {
                    return None;
                }
                QUERY_VECTORS.put(key, query, embedded.clone());
                embedded
            }
        };
    Some((
        crate::store::operator_turns::vector_to_text_literal(&vector),
        key.to_owned(),
        service.model_name(),
    ))
}

pub(crate) struct Retrieved {
    pub(crate) answer: Value,
    pub(crate) passages: Passages,
}

pub(crate) async fn retrieve_page(
    state: &AppState,
    input: &KnowledgeQuery,
) -> Result<Retrieved, Response> {
    retrieve_with(state, input, true).await
}

pub(crate) async fn retrieve_without_rerank(
    state: &AppState,
    input: &KnowledgeQuery,
) -> Result<Value, Response> {
    retrieve_with(state, input, false)
        .await
        .map(|found| found.answer)
}

async fn embed_for(
    service: &EmbeddingService,
    key: &str,
    query: &str,
) -> (Option<QueryVector>, Option<&'static str>) {
    if query.trim().is_empty() {
        return (None, Some("empty_query"));
    }
    if !service.is_available() {
        return (None, Some("embedding_disabled"));
    }
    match query_vector(service, key, query).await {
        Some(vector) => (Some(vector), None),
        None => (None, Some("embedding_unavailable")),
    }
}

type Searched = (Value, Option<RerankReport>, Passages);

async fn search_stage(
    state: &AppState,
    handle: &PersistHandle,
    project: &ProjectIdentity,
    (input, vector): (&KnowledgeQuery, Option<&QueryVector>),
    rerank: bool,
) -> Result<Searched, PersistError> {
    if rerank {
        let (page, report, passages) =
            super::rerank_page::search(state, handle, project, input, vector).await?;
        return Ok((page, Some(report), passages));
    }
    let page = handle
        .search_resolved_knowledge(project, input, vector)
        .await?;
    Ok((page, None, Passages::new()))
}

async fn retrieve_with(
    state: &AppState,
    input: &KnowledgeQuery,
    rerank: bool,
) -> Result<Retrieved, Response> {
    let input = &KnowledgeQuery {
        semantic_cut: state.rerank_service().ranking().semantic_cut(),
        ..input.clone()
    };
    let handle = state
        .persist()
        .ok_or_else(crate::api::handlers::persist_unavailable_response)?;
    let service = state.embedding_service();
    let key = model_key(&service);
    let (vector, fallback) = embed_for(&service, &key, &input.query).await;
    let project = handle
        .resolve_knowledge_project(&input.project)
        .await
        .map_err(knowledge_error)?;
    let searching = search_stage(state, handle, &project, (input, vector.as_ref()), rerank);
    let counting = handle.knowledge_coverage(project.id, &key);
    let (searched, coverage) = tokio::join!(searching, counting);
    let (mut answer, report, passages) = searched.map_err(knowledge_error)?;
    let coverage = coverage.map_err(knowledge_error)?;
    answer["embedding"] = json!({"coverage":coverage, "model":service.model_name(), "fallback":fallback,
        "scope":"artifact_lesson_note", "turns":"existing model-matched turn embeddings"});
    if let Some(report) = report {
        answer["rerank"] = report.to_json();
    }
    Ok(Retrieved { answer, passages })
}
