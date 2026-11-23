use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::base_state::BaseState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime_repo::RuntimePersist as _;

#[derive(Default, Deserialize)]
pub struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn list(
    State(state): State<BaseState>,
    Path(session): Path<Uuid>,
    Query(page): Query<Page>,
) -> Response {
    let Some(persist) = state.persist() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let limit = page.limit.unwrap_or(20).clamp(1, 100);
    let offset = page.offset.unwrap_or(0).max(0);
    match persist
        .runtime()
        .list_capture_exchanges(session, limit, offset)
        .await
    {
        Ok(exchanges) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(serde_json::json!({
                "exchanges":exchanges,"next_offset":offset + limit
            })),
        )
            .into_response(),
        Err(error) => failure(error),
    }
}

pub async fn body(
    State(state): State<BaseState>,
    Path((session, exchange, direction)): Path<(Uuid, Uuid, String)>,
) -> Response {
    let Some(persist) = state.persist() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let capture = match persist.runtime().get_capture_exchange(exchange).await {
        Ok(Some(capture)) if capture.session_uuid == Some(session) => capture,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return failure(error),
    };
    let document = match direction.as_str() {
        "request" => Some(capture.request_document_id),
        "sent" => Some(capture.sent_document_id),
        "response" => capture.response_document_id,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let Some(document) = document else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let document = match persist.runtime().capture_document(document).await {
        Ok(Some(document)) => document,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return failure(error),
    };
    let mut response = Response::new(stream_body(persist.clone(), document.id));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "x-mike-capture-complete",
        HeaderValue::from_static(if document.complete { "true" } else { "false" }),
    );
    if document.finished
        && let Ok(length) = document.byte_size.to_string().parse()
    {
        headers.insert(header::CONTENT_LENGTH, length);
    }
    response
}

fn stream_body(persist: PersistHandle, document: i64) -> Body {
    let initial = (
        persist,
        0_i64,
        std::collections::VecDeque::<axum::body::Bytes>::new(),
    );
    let stream = futures::stream::try_unfold(
        initial,
        move |(persist, mut offset, mut buffered)| async move {
            if buffered.is_empty() {
                let chunks = persist
                    .runtime()
                    .capture_document_chunks(document, offset, 32)
                    .await
                    .map_err(std::io::Error::other)?;
                offset += chunks.len() as i64;
                buffered.extend(
                    chunks
                        .into_iter()
                        .map(|chunk| axum::body::Bytes::from(chunk.body)),
                );
            }
            Ok::<_, std::io::Error>(
                buffered
                    .pop_front()
                    .map(|bytes| (bytes, (persist, offset, buffered))),
            )
        },
    );
    Body::from_stream(stream)
}

fn failure(error: impl std::fmt::Display) -> Response {
    tracing::warn!(error = %error, "Runtime capture read failed");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}
