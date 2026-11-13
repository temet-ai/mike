use crate::store::persistence::runtime_repo::RuntimePersist as _;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::base_state::BaseState;
use crate::runtime::codex_observe::SUPPORTED_CODEX_VERSIONS;
use crate::runtime::codex_observe::ingest::{self, IngestError, ObserveSource};
use mike_codex::observe::COVERAGE_GAPS;

pub const SOURCE_HEADER: &str = "x-mike-codex-source";
pub const VERSION_HEADER: &str = "x-mike-codex-version";
pub const DELIVERY_HEADER: &str = "x-mike-delivery-id";
const DEFAULT_SOURCE: &str = "unlabelled";
const MAX_PAGE: i64 = 200;

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
        hook_boundary: header_text(headers, "x-mike-codex-control").as_deref()
            == Some("hook_boundary"),
        codex_home: header_text(headers, "x-mike-codex-home"),
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

pub async fn post_hook(
    State(state): State<BaseState>,
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

pub async fn post_otel(
    State(state): State<BaseState>,
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
pub struct ListQuery {
    session_id: Option<Uuid>,
    root_thread_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn list_observed(
    State(state): State<BaseState>,
    Query(query): Query<ListQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled").into_response();
    };
    let limit = query.limit.unwrap_or(100).clamp(1, MAX_PAGE);
    let offset = query.offset.unwrap_or(0).max(0);
    let selected = query.session_id.or_else(|| {
        query
            .root_thread_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
    });
    if query.root_thread_id.is_some() && selected.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            "use the canonical Mike session_id returned by hook ingestion",
        )
            .into_response();
    }
    let sessions = match selected {
        Some(session) => persist.runtime().session_observations(&[session]).await,
        None => {
            persist
                .runtime()
                .list_runtime_sessions("codex", limit, offset)
                .await
        }
    };
    let sessions = match sessions {
        Ok(rows) => rows,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let items = match selected {
        Some(session) => match persist
            .runtime()
            .list_runtime_items(session, limit, offset)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
            }
        },
        None => Vec::new(),
    };
    Json(json!({
        "sessions": sessions,
        "items": items,
        "next_offset": offset + limit,
        "items_scope": "supply session_id for paginated turn and tool metadata",
        "content_availability": "not_collected",
        "capture": "normalized lifecycle, turn and tool metadata; raw hook and telemetry payloads are not retained",
        "supported_codex_versions": SUPPORTED_CODEX_VERSIONS,
        "coverage_gaps": COVERAGE_GAPS,
    }))
    .into_response()
}
