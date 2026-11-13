use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use chrono::Utc;
use futures::StreamExt;
use mike_codex::responses::ResponsesSummary;
use mike_codex::responses_body::BodyCapture;
use std::sync::{Arc, Mutex};

use super::record::{ExchangeRecord, ExchangeStatus, Finished, spawn_finish};
use super::request_body::{RequestBody, SharedCapture};
use super::tee::Tee;
use super::{AuthRoute, ROUTE_PREFIX, ResponsesState, correlate, headers};
use crate::extract::headers::{filter_request_headers, filter_response_headers};

const REFUSED_WEBSOCKET: &str = "mike captures Codex Responses traffic over HTTP; the WebSocket transport is refused so Codex falls back to HTTP/SSE";

pub async fn handle(State(state): State<ResponsesState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let expected = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let slot = state.slots.clone().try_acquire_owned().ok();
    let enabled = state.toggles.load().codex_diagnostic_capture && slot.is_some();
    let capture = Arc::new(Mutex::new(BodyCapture::new(if enabled {
        super::MAX_DIAGNOSTIC_BYTES
    } else {
        0
    })));
    let route = headers::auth_route(&parts.headers);
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());
    let upstream_url = upstream_url(state.upstreams.base_for(route), &path_and_query);
    let record = base_record(
        &parts.method,
        &parts.headers,
        &path_and_query,
        upstream_url,
        route,
        capture.clone(),
    );
    let mut record = record;
    record.slot = slot;
    record.diagnostics_enabled = enabled;
    if headers::is_websocket_upgrade(&parts.headers) {
        return refuse_websocket(state, record);
    }
    let body = reqwest::Body::wrap_stream(RequestBody::new(
        body.into_data_stream().boxed(),
        capture,
        expected,
    ));
    forward(state, record, parts.method, &parts.headers, body).await
}

pub fn upstream_url(base: &str, path_and_query: &str) -> String {
    let rest = path_and_query
        .strip_prefix(ROUTE_PREFIX)
        .unwrap_or(path_and_query);
    format!("{}{}", base.trim_end_matches('/'), rest)
}

fn base_record(
    method: &axum::http::Method,
    request_headers: &HeaderMap,
    path: &str,
    upstream_url: String,
    auth_route: AuthRoute,
    capture: SharedCapture,
) -> ExchangeRecord {
    let encoding = headers::text(request_headers, "content-encoding");
    ExchangeRecord {
        correlation: correlate(request_headers, &[]),
        auth_route,
        websocket_refused: false,
        method: method.as_str().to_string(),
        path: path.to_string(),
        upstream_url,
        request_headers: headers::captured(request_headers),
        forwarded_headers: serde_json::Value::Array(Vec::new()),
        model: None,
        request_body_encoding: encoding,
        request_body: capture,
        slot: None,
        diagnostics_enabled: false,
        codex_home: headers::text(request_headers, "x-mike-codex-home")
            .filter(|home| home.len() <= 4096)
            .map(Into::into),
        response_status: None,
        response_headers: None,
        response_is_event_stream: false,
        upstream_request_id: None,
        created_at: Utc::now(),
    }
}

fn refuse_websocket(state: ResponsesState, mut record: ExchangeRecord) -> Response {
    record.websocket_refused = true;
    record.response_status = Some(StatusCode::UPGRADE_REQUIRED.as_u16());
    let finished = Finished {
        status: ExchangeStatus::Refused,
        response_body: None,
        summary: ResponsesSummary::default(),
        evidence_complete: true,
        evidence_refused: false,
        error: Some(REFUSED_WEBSOCKET.to_string()),
    };
    spawn_finish(state, record, finished);
    plain(StatusCode::UPGRADE_REQUIRED, REFUSED_WEBSOCKET)
}

fn upstream_host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| u.port().map_or(h.to_string(), |p| format!("{h}:{p}")))
        })
        .unwrap_or_default()
}

async fn forward(
    state: ResponsesState,
    mut record: ExchangeRecord,
    method: axum::http::Method,
    inbound: &HeaderMap,
    body: reqwest::Body,
) -> Response {
    let outbound = filter_request_headers(inbound, &upstream_host(&record.upstream_url));
    record.forwarded_headers = headers::captured(&outbound);
    let sent = state
        .client
        .request(method, &record.upstream_url)
        .headers(outbound)
        .body(body)
        .send()
        .await;
    match sent {
        Ok(upstream) => relay_response(state, record, upstream),
        Err(error) => {
            let message = format!("upstream request failed: {error}");
            let finished = Finished {
                status: ExchangeStatus::Error,
                response_body: None,
                summary: ResponsesSummary::default(),
                evidence_complete: false,
                evidence_refused: false,
                error: Some(message.clone()),
            };
            spawn_finish(state, record, finished);
            plain(StatusCode::BAD_GATEWAY, &message)
        }
    }
}

fn relay_response(
    state: ResponsesState,
    mut record: ExchangeRecord,
    upstream: reqwest::Response,
) -> Response {
    let status = upstream.status();
    let response_headers = filter_response_headers(upstream.headers());
    record.response_status = Some(status.as_u16());
    record.response_headers = Some(headers::captured(upstream.headers()));
    record.upstream_request_id = headers::upstream_request_id(upstream.headers());
    record.response_is_event_stream = headers::text(upstream.headers(), "content-type")
        .is_some_and(|ct| ct.contains("text/event-stream"));
    let expected = upstream.content_length();
    let tee = Tee::new(upstream.bytes_stream().boxed(), state, record, expected);
    let mut response = Response::new(Body::from_stream(tee));
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    response
}

fn plain(status: StatusCode, message: &str) -> Response {
    let mut response = Response::new(Body::from(message.to_string()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
