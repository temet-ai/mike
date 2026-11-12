use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::config::AppState;
use crate::runtime::codex_observe::ingest::{self, IngestError, ObserveSource};
use crate::runtime::codex_observe::{SUPPORTED_CODEX_VERSIONS, reads, tool_kind};

pub(crate) const SOURCE_HEADER: &str = "x-mike-codex-source";
pub(crate) const VERSION_HEADER: &str = "x-mike-codex-version";
pub(crate) const DELIVERY_HEADER: &str = "x-mike-delivery-id";
const DEFAULT_SOURCE: &str = "unlabelled";
const MAX_PAGE: i64 = 500;

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('\0'))
        .map(str::to_owned)
}

fn source(headers: &HeaderMap) -> ObserveSource {
    ObserveSource {
        label: header_text(headers, SOURCE_HEADER).unwrap_or_else(|| DEFAULT_SOURCE.to_owned()),
        codex_version: header_text(headers, VERSION_HEADER),
    }
}

fn delivery_id(headers: &HeaderMap) -> Result<Uuid, String> {
    match header_text(headers, DELIVERY_HEADER) {
        None => Ok(Uuid::new_v4()),
        Some(text) => Uuid::parse_str(&text)
            .ok()
            .filter(|id| !id.is_nil())
            .ok_or_else(|| format!("{DELIVERY_HEADER} {text:?} is not a non-nil UUID")),
    }
}

fn answer(result: Result<ingest::IngestReport, IngestError>) -> Response {
    match result {
        Ok(report) => Json(report).into_response(),
        Err(IngestError::BadInput(message)) => (StatusCode::BAD_REQUEST, message).into_response(),
        Err(IngestError::Store(message)) => {
            tracing::warn!(error = %message, "codex observe ingest failed");
            (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
        }
    }
}

pub(crate) async fn post_hook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(persist) = state.persist() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled").into_response();
    };
    let delivery = match delivery_id(&headers) {
        Ok(id) => id,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    answer(ingest::ingest_hook(persist, &source(&headers), delivery, &body).await)
}

pub(crate) async fn post_otel(
    State(state): State<AppState>,
    Path(signal): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(persist) = state.persist() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled").into_response();
    };
    if !matches!(signal.as_str(), "logs" | "traces" | "metrics") {
        let message = format!("OTLP signal {signal:?} is not one of logs, traces, metrics");
        return (StatusCode::NOT_FOUND, message).into_response();
    }
    let content_type = header_text(&headers, header::CONTENT_TYPE.as_str()).unwrap_or_default();
    if content_type.contains("protobuf") {
        let message = format!(
            "content-type {content_type:?}: mike reads OTLP/JSON only; set protocol = \"json\" on the Codex otlp-http exporter"
        );
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, message).into_response();
    }
    answer(ingest::ingest_otel(persist, &source(&headers), &signal, &body).await)
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    root_thread_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(crate) async fn list_observed(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled").into_response();
    };
    let limit = query.limit.unwrap_or(100).clamp(1, MAX_PAGE);
    let offset = query.offset.unwrap_or(0).max(0);
    let threads = match reads::list_threads(
        persist.pool(),
        query.root_thread_id.as_deref(),
        limit,
        offset,
    )
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let mut tools = Vec::new();
    for namespace_id in threads
        .iter()
        .map(|t| t.namespace_id)
        .collect::<std::collections::BTreeSet<_>>()
    {
        let ids: Vec<String> = threads
            .iter()
            .filter(|t| t.namespace_id == namespace_id)
            .map(|t| t.thread_id.clone())
            .collect();
        match reads::list_tools(persist.pool(), namespace_id, &ids).await {
            Ok(rows) => tools.extend(rows),
            Err(error) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
            }
        }
    }
    Json(json!({
        "threads": threads,
        "tools": tools,
        "next_offset": offset + limit,
        "control": "none",
        "capture": "observed: hook payloads and telemetry summaries, not raw model traffic",
        "supported_codex_versions": SUPPORTED_CODEX_VERSIONS,
        "coverage_gaps": tool_kind::COVERAGE_GAPS,
    }))
    .into_response()
}
