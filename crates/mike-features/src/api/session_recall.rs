use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use uuid::Uuid;

use crate::api::client_serves::by_deadline;
use crate::recall_injector::{self, RecallAnswer, RecallInjectorConfig};
use crate::state::AppState;

const RECALL_BUDGET: Duration = Duration::from_millis(200);

#[derive(Debug, Deserialize)]
pub struct RecallRequest {
    project: String,
    topics: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RecallResponse {
    topics: Vec<String>,
    lesson_ids: Vec<i64>,
    text: Option<String>,
}

pub async fn post_session_recall(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RecallRequest>,
) -> Result<Json<RecallResponse>, StatusCode> {
    if Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    crate::api::client_serves::require_client_serves(&state, "client_serves_recall")?;
    let toggles = state.resolve_session_toggles(&id);
    let cfg = RecallInjectorConfig {
        enabled: toggles.recall_injector,
        max_lessons_per_topic: toggles.recall_injector_max_lessons_per_topic,
    };
    let answer = match state.persist.clone() {
        Some(persist) => {
            let (project, topics) = (request.project.clone(), request.topics.clone());
            let deadline = Instant::now() + RECALL_BUDGET;
            let answer = by_deadline(deadline, async move {
                recall_injector::answer_recall(&persist, &project, &topics, &cfg).await
            })
            .await;
            let Some(answer) = answer else {
                tracing::info!(session_id = %id, "recall: lessons not read inside the budget; the proxy recalls for this turn");
                return Ok(Json(RecallResponse {
                    topics: request.topics,
                    lesson_ids: Vec::new(),
                    text: None,
                }));
            };
            answer
        }
        None => RecallAnswer::default(),
    };
    recall_injector::mark_served(&id);
    Ok(Json(RecallResponse {
        topics: request.topics,
        lesson_ids: answer.lesson_ids,
        text: answer.text,
    }))
}

#[cfg(test)]
pub mod tests;
