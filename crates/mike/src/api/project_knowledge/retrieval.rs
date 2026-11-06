use super::knowledge_error;
use crate::config::AppState;
use crate::embed_worker::knowledge::{model_key, valid_vector};
use crate::store::persistence::project_knowledge::KnowledgeQuery;
use axum::response::Response;
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) async fn retrieve(state: &AppState, input: &KnowledgeQuery) -> Result<Value, Response> {
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
        match tokio::time::timeout(Duration::from_secs(3), service.embed_query(&input.query)).await
        {
            Ok(Ok(vector))
                if valid_vector(&vector, service.dim()) && model_key(&service) == key =>
            {
                Some((
                    crate::store::operator_turns::vector_to_text_literal(&vector),
                    key.clone(),
                    service.model_name(),
                ))
            }
            _ => {
                fallback = Some("embedding_unavailable");
                None
            }
        }
    };
    let project = handle
        .resolve_knowledge_project(&input.project)
        .await
        .map_err(knowledge_error)?;
    let mut result = if vector.is_some() {
        handle
            .search_project_knowledge_hybrid(input, vector.as_ref())
            .await
    } else {
        handle.search_project_knowledge(input).await
    }
    .map_err(knowledge_error)?;
    let coverage = handle
        .knowledge_embedding_health(Some(project.id), &key)
        .await
        .map_err(knowledge_error)?;
    result["embedding"] = json!({"coverage":coverage, "model":service.model_name(), "fallback":fallback,
        "scope":"artifact_lesson_note", "turns":"existing model-matched turn embeddings"});
    Ok(result)
}
