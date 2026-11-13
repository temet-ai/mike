use axum::response::Response;

use crate::error::ProxyError;
use crate::feature_toggles::ResolvedToggles;
use crate::in_flight::InFlightGuard;
use crate::state::AppState;
use crate::store::models::RequestId;

pub mod buffered;
pub mod streaming;

pub struct ResponseCtx {
    pub state: AppState,
    pub request_id: RequestId,
    pub status: reqwest::StatusCode,
    pub stored_resp_headers: Vec<(String, String)>,
    pub upstream_request_id: Option<String>,
    pub session_toggles: std::sync::Arc<ResolvedToggles>,
    pub upstream_resp: reqwest::Response,
    pub upstream_url: String,
    pub forwarded_headers_for_correction: axum::http::HeaderMap,
    pub upstream_body_for_correction: bytes::Bytes,
    pub session_id: String,
    pub in_flight_guard: InFlightGuard,
    pub handler_start: std::time::Instant,
    pub streaming_model: Option<String>,
    pub resp_headers: axum::http::HeaderMap,
    pub is_sse_response: bool,
    pub project_path: String,
    pub proxy_tools_declared: bool,
    pub context_focus_asked: bool,
}

pub async fn forward(ctx: ResponseCtx) -> Result<Response, ProxyError> {
    if ctx.is_sse_response {
        streaming::forward_streaming(ctx).await
    } else {
        buffered::forward_buffered(ctx).await
    }
}
