use axum::Json;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::base_state::BaseState;
use crate::store::persistence::runtime_repo::RuntimePersist as _;

#[derive(Deserialize)]
pub struct SessionQuery {
    thread_id: String,
}

pub async fn get(State(state): State<BaseState>, Query(query): Query<SessionQuery>) -> Response {
    if query.thread_id.trim().is_empty()
        || query.thread_id.len() > 512
        || query.thread_id.contains('\0')
    {
        return failure(
            StatusCode::BAD_REQUEST,
            "invalid_thread_id",
            "thread_id must be nonblank, at most 512 bytes, and contain no NUL",
        );
    }
    let Some(persist) = state.persist() else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "Codex session identity requires persistence",
        );
    };
    match persist
        .runtime()
        .resolve_unique_codex_session(&query.thread_id)
        .await
    {
        Ok(Some(session_id)) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"session_id": session_id})),
        )
            .into_response(),
        Ok(None) => failure(
            StatusCode::NOT_FOUND,
            "codex_session_unresolved",
            "thread_id has no unique Codex session binding",
        ),
        Err(error) => {
            tracing::warn!(%error, "Codex caller identity lookup failed");
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "codex_session_lookup_failed",
                "Codex session identity could not be read",
            )
        }
    }
}

fn failure(status: StatusCode, error: &str, message: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"error": error, "message": message})),
    )
        .into_response()
}
