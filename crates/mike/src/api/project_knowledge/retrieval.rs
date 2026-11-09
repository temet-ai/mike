use super::knowledge_error;
use super::query_vector_cache::QUERY_VECTORS;
use crate::analysis::EmbeddingService;
use crate::config::AppState;
use crate::embed_worker::knowledge::{model_key, valid_vector};
use crate::store::persistence::project_knowledge::KnowledgeQuery;
use axum::response::Response;
use serde_json::{Value, json};
use std::time::Duration;

async fn query_vector(
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

pub(crate) async fn retrieve(state: &AppState, input: &KnowledgeQuery) -> Result<Value, Response> {
    retrieve_with(state, input, true).await
}

pub(crate) async fn retrieve_without_rerank(
    state: &AppState,
    input: &KnowledgeQuery,
) -> Result<Value, Response> {
    retrieve_with(state, input, false).await
}

async fn retrieve_with(
    state: &AppState,
    input: &KnowledgeQuery,
    rerank: bool,
) -> Result<Value, Response> {
    let handle = state
        .persist()
        .ok_or_else(crate::api::handlers::persist_unavailable_response)?;
    let service = state.embedding_service();
    let key = model_key(&service);
    let mut fallback = None;
    let vector = if input.query.trim().is_empty() || !service.is_available() {
        fallback = Some(if input.query.trim().is_empty() {
            "empty_query"
        } else {
            "embedding_disabled"
        });
        None
    } else {
        let embedded = query_vector(&service, &key, &input.query).await;
        if embedded.is_none() {
            fallback = Some("embedding_unavailable");
        }
        embedded
    };
    let project = handle
        .resolve_knowledge_project(&input.project)
        .await
        .map_err(knowledge_error)?;
    let (mut result, report) = if rerank {
        super::rerank_page::search(state, handle, &project, input, vector.as_ref())
            .await
            .map(|(page, report)| (page, Some(report)))
    } else {
        handle
            .search_project_knowledge_hybrid(input, vector.as_ref())
            .await
            .map(|page| (page, None))
    }
    .map_err(knowledge_error)?;
    let coverage = handle
        .knowledge_embedding_health(Some(project.id), &key)
        .await
        .map_err(knowledge_error)?;
    result["embedding"] = json!({"coverage":coverage, "model":service.model_name(), "fallback":fallback,
        "scope":"artifact_lesson_note", "turns":"existing model-matched turn embeddings"});
    if let Some(report) = report {
        result["rerank"] = report.to_json();
    }
    Ok(result)
}
