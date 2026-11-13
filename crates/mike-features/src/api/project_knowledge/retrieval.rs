use super::knowledge_error;
use super::passage_excerpt::Passages;
use super::query_vector_cache::QUERY_VECTORS;
use super::rerank_page::QueryVector;
use crate::analysis::EmbeddingService;
use crate::api::rerank_search::RerankReport;
use crate::embed_worker::knowledge::{model_key, model_keys, valid_vector};
use crate::semantic::KnowledgeState;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, ProjectIdentity};
use crate::store::persistence::{PersistError, PersistHandle};
use axum::response::Response;
use serde_json::{Value, json};
use std::time::Duration;

pub async fn query_vector(
    service: &EmbeddingService,
    key: &str,
    query: &str,
) -> Option<QueryVector> {
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
        model_keys(service),
        service.model_name(),
    ))
}

pub struct Retrieved {
    pub answer: Value,
    pub passages: Passages,
}

pub async fn retrieve_page(
    state: &KnowledgeState,
    input: &KnowledgeQuery,
) -> Result<Retrieved, Response> {
    retrieve_with(state, input, true).await
}

pub async fn retrieve_without_rerank(
    state: &KnowledgeState,
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
    state: &KnowledgeState,
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
        .knowledge()
        .search_resolved_knowledge(project, input, vector)
        .await?;
    Ok((page, None, Passages::new()))
}

async fn retrieve_with(
    state: &KnowledgeState,
    input: &KnowledgeQuery,
    rerank: bool,
) -> Result<Retrieved, Response> {
    let input = &KnowledgeQuery {
        semantic_cut: state.rerank_service().ranking().semantic_cut(),
        ..input.clone()
    };
    let handle = state
        .persist()
        .ok_or_else(crate::web_base::persist_unavailable_response)?;
    let service = state.embedding_service();
    let key = model_key(&service);
    let (vector, fallback) = embed_for(&service, &key, &input.query).await;
    let project = handle
        .knowledge()
        .resolve_knowledge_project(&input.project)
        .await
        .map_err(knowledge_error)?;
    let searching = search_stage(state, handle, &project, (input, vector.as_ref()), rerank);
    let keys = model_keys(&service);
    let counting = handle.knowledge().knowledge_coverage(project.id, &keys);
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
