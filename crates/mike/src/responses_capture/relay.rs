use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use chrono::Utc;
use futures::StreamExt;

use super::record::{ExchangeRecord, ExchangeStatus, Finished, request_model, spawn_finish};
use super::tee::Tee;
use super::{AuthRoute, ROUTE_PREFIX, ResponsesState, correlate, headers};
use crate::proxy::headers::{filter_request_headers, filter_response_headers};

const REFUSED_WEBSOCKET: &str = "mike captures Codex Responses traffic over HTTP; the WebSocket transport is refused so Codex falls back to HTTP/SSE";

pub(super) async fn handle(State(state): State<ResponsesState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let body = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return plain(
                StatusCode::BAD_REQUEST,
                &format!("request body unreadable: {error}"),
            );
        }
    };
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
        body,
    );
    if headers::is_websocket_upgrade(&parts.headers) {
        return refuse_websocket(state, record);
    }
    forward(state, record, parts.method, &parts.headers).await
}

pub(super) fn upstream_url(base: &str, path_and_query: &str) -> String {
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
    body: Bytes,
) -> ExchangeRecord {
    let encoding = headers::text(request_headers, "content-encoding");
    ExchangeRecord {
        correlation: correlate(request_headers, &body),
        auth_route,
        websocket_refused: false,
        method: method.as_str().to_string(),
        path: path.to_string(),
        upstream_url,
        request_headers: headers::captured(request_headers),
        forwarded_headers: serde_json::Value::Array(Vec::new()),
        model: request_model(&body, encoding.as_deref()),
        request_body_encoding: encoding,
        forwarded_body: body.clone(),
        request_body: body,
        response_status: None,
        response_headers: None,
        response_is_event_stream: false,
        upstream_request_id: None,
        created_at: Utc::now(),
    }
}

fn refuse_websocket(state: ResponsesState, mut record: ExchangeRecord) -> Response {
    record.websocket_refused = true;
    record.forwarded_body = Bytes::new();
    record.response_status = Some(StatusCode::UPGRADE_REQUIRED.as_u16());
    let finished = Finished {
        status: ExchangeStatus::Refused,
        response_body: None,
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
) -> Response {
    let outbound = filter_request_headers(inbound, &upstream_host(&record.upstream_url));
    record.forwarded_headers = headers::captured(&outbound);
    let sent = state
        .client
        .request(method, &record.upstream_url)
        .headers(outbound)
        .body(record.forwarded_body.clone())
        .send()
        .await;
    match sent {
        Ok(upstream) => relay_response(state, record, upstream),
        Err(error) => {
            let message = format!("upstream request failed: {error}");
            let finished = Finished {
                status: ExchangeStatus::Error,
                response_body: None,
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
    let tee = Tee::new(upstream.bytes_stream().boxed(), state, record);
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
