use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, Request, Uri};
use axum::response::Response;
use bytes::Bytes;

use crate::config::AppState;
use crate::error::ProxyError;
use crate::extract::signals::RequestSignals;
use crate::feature_toggles::ResolvedToggles;
use crate::in_flight::InFlightGuard;
use crate::metrics::{COUNTER_REQUESTS_TOTAL, HISTOGRAM_UPSTREAM_LATENCY, status_bucket};
use crate::store::models::{RequestId, SystemReminder};

mod attribution;
mod capture_prep;
mod capture_register;
pub(crate) mod channel_link;
mod finalize;
mod guards;
mod mutation_stages;
mod request_pipeline;
pub(crate) mod workspace_location;
pub(crate) mod workspace_register;
mod workspace_repo;

pub(crate) use finalize::finalize_and_respond;
pub(crate) use request_pipeline::run_request_pipeline;

pub(crate) struct AcceptedRequest {
    pub(crate) parts: Parts,
    pub(crate) body_bytes: Bytes,
    pub(crate) signals: RequestSignals,
    pub(crate) session_toggles: Arc<ResolvedToggles>,
    pub(crate) stored_body: Option<Bytes>,
}

pub(crate) enum AcceptOutcome {
    Accepted(Box<AcceptedRequest>),
    EarlyResponse(Response),
}

fn uncaptured_log_fields(headers: &axum::http::HeaderMap) -> (&str, &str) {
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let x_app = headers
        .get("x-app")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    (user_agent, x_app)
}

async fn forward_sessionless(
    state: &AppState,
    parts: Parts,
    body_bytes: Bytes,
    miss: crate::extract::request::SessionIdMiss,
) -> Result<Response, ProxyError> {
    use crate::extract::request::SessionIdMiss;
    let reason = match miss {
        SessionIdMiss::Missing => "missing_session_header",
        SessionIdMiss::Malformed => "malformed_session_header",
        SessionIdMiss::NonUtf8 => "non_utf8_session_header",
    };
    if !matches!(miss, SessionIdMiss::Missing) {
        let (user_agent, x_app) = uncaptured_log_fields(&parts.headers);
        let offending_header = parts
            .headers
            .get("x-claude-code-session-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        tracing::warn!(
            %reason,
            path = %parts.uri.path(),
            method = %parts.method,
            %user_agent,
            %x_app,
            %offending_header,
            "request with an unusable session id; captured as sessionless"
        );
    }
    super::sessionless::forward_and_capture(
        state,
        reason,
        parts.method,
        &parts.uri,
        &parts.headers,
        body_bytes,
    )
    .await
}

pub(crate) async fn accept(
    state: &AppState,
    req: Request<Body>,
) -> Result<AcceptOutcome, ProxyError> {
    state.inc_request_count();

    let (parts, body) = req.into_parts();
    let max_body = state.config.max_request_body_bytes;
    let max_body_usize = usize::try_from(max_body).unwrap_or(usize::MAX);
    let body_bytes = match axum::body::to_bytes(body, max_body_usize).await {
        Ok(bytes) => bytes,
        Err(err) => {
            if super::error_response::is_length_limit_error(&err) {
                return Ok(AcceptOutcome::EarlyResponse(
                    super::error_response::payload_too_large(max_body),
                ));
            }
            return Err(ProxyError::BodyRead(err.to_string()));
        }
    };

    if let Some(route) = super::remote_control::recognise(parts.uri.path()) {
        let response = super::remote_control::capture::forward_and_capture(
            state,
            &route,
            parts.method.clone(),
            &parts.uri,
            &parts.headers,
            body_bytes,
        )
        .await?;
        return Ok(AcceptOutcome::EarlyResponse(response));
    }

    let signals = match RequestSignals::from_parts_and_body(&parts, &body_bytes, state).await {
        Ok(signals) => signals,
        Err(miss) => {
            let response = forward_sessionless(state, parts, body_bytes, miss).await?;
            return Ok(AcceptOutcome::EarlyResponse(response));
        }
    };

    let session_toggles = state.resolve_session_toggles(&signals.session_id);

    channel_link::maybe_link_channel(state, &signals.session_id, &body_bytes);

    workspace_register::maybe_register_workspace_session(
        state,
        &signals.session_id,
        &signals.project_path,
        session_toggles.workspace_capture_registration,
    );

    state.note_capturable_request();
    let stored_body = Some(body_bytes.clone());

    Ok(AcceptOutcome::Accepted(Box::new(AcceptedRequest {
        parts,
        body_bytes,
        signals,
        session_toggles,
        stored_body,
    })))
}

pub(crate) struct PipelineResult {
    pub(crate) parts: Parts,
    pub(crate) upstream_body: Bytes,
    pub(crate) request_id: RequestId,
    pub(crate) session_id: String,
    pub(crate) session_toggles: Arc<ResolvedToggles>,
    pub(crate) in_flight_guard: InFlightGuard,
    pub(crate) streaming_model: Option<String>,
    pub(crate) injected_reminders: Vec<SystemReminder>,
    pub(crate) observed_composition_applied: bool,
    pub(crate) composed_system_text: Option<String>,
    pub(crate) project_path: String,
    pub(crate) carrier_eligible: bool,
    pub(crate) proxy_tools_declared: bool,
    pub(crate) context_focus_asked: bool,
}

pub(crate) enum PipelineOutcome {
    Continue(Box<PipelineResult>),
    EarlyResponse(Response),
}

pub(crate) struct ForwardOutcome {
    pub(crate) upstream_resp: reqwest::Response,
    pub(crate) status: reqwest::StatusCode,
    pub(crate) resp_headers: HeaderMap,
    pub(crate) is_sse_response: bool,
    pub(crate) upstream_url: String,
    pub(crate) forwarded_headers_for_correction: HeaderMap,
    pub(crate) upstream_body_for_correction: Bytes,
}

pub(crate) async fn forward_to_upstream(
    state: &AppState,
    request_id: RequestId,
    method: Method,
    uri: &Uri,
    headers: &HeaderMap,
    upstream_body: Bytes,
) -> Result<ForwardOutcome, ProxyError> {
    let upstream_url =
        super::upstream::build_upstream_url(&state.config.upstream_url, uri.path(), uri.query());
    let upstream_host = super::upstream::extract_host(&state.config.upstream_url);
    let req_headers = crate::extract::headers::filter_request_headers(headers, &upstream_host);
    let forwarded_headers_for_correction = req_headers.clone();

    let upstream_body_for_correction = upstream_body.clone();
    let upstream_started_at = Instant::now();
    let upstream_req = state
        .client
        .request(method, &upstream_url)
        .headers(req_headers)
        .body(upstream_body)
        .build()
        .map_err(|e| ProxyError::Internal(format!("failed to build upstream request: {e}")))?;
    let upstream_resp = super::upstream::send_with_retry(&state.client, upstream_req)
        .await
        .map_err(ProxyError::UpstreamConnect)?;
    metrics::histogram!(HISTOGRAM_UPSTREAM_LATENCY)
        .record(upstream_started_at.elapsed().as_secs_f64());

    let status = upstream_resp.status();
    metrics::counter!(
        COUNTER_REQUESTS_TOTAL,
        "status" => status_bucket(status.as_u16())
    )
    .increment(1);
    let resp_headers = crate::extract::headers::filter_response_headers(upstream_resp.headers());

    let is_sse_response = resp_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.starts_with("text/event-stream"))
        .unwrap_or(false);

    tracing::info!(
        request_id = %request_id,
        status = status.as_u16(),
        sse = is_sse_response,
        "upstream responded"
    );

    Ok(ForwardOutcome {
        upstream_resp,
        status,
        resp_headers,
        is_sse_response,
        upstream_url,
        forwarded_headers_for_correction,
        upstream_body_for_correction,
    })
}
