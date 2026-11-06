use axum::response::Response;

use crate::config::AppState;
use crate::error::ProxyError;
use crate::feature_toggles::ResolvedToggles;
use crate::in_flight::InFlightGuard;
use crate::store::models::RequestId;

mod buffered;
mod streaming;

pub(crate) struct ResponseCtx {
    pub(crate) state: AppState,
    pub(crate) request_id: RequestId,
    pub(crate) status: reqwest::StatusCode,
    pub(crate) stored_resp_headers: Vec<(String, String)>,
    pub(crate) upstream_request_id: Option<String>,
    pub(crate) session_toggles: std::sync::Arc<ResolvedToggles>,
    pub(crate) upstream_resp: reqwest::Response,
    pub(crate) upstream_url: String,
    pub(crate) forwarded_headers_for_correction: axum::http::HeaderMap,
    pub(crate) upstream_body_for_correction: bytes::Bytes,
    pub(crate) session_id: String,
    pub(crate) in_flight_guard: InFlightGuard,
    pub(crate) handler_start: std::time::Instant,
    pub(crate) streaming_model: Option<String>,
    pub(crate) resp_headers: axum::http::HeaderMap,
    pub(crate) is_sse_response: bool,
    pub(crate) project_path: String,
    pub(crate) proxy_tools_declared: bool,
    pub(crate) context_focus_asked: bool,
}

pub(crate) async fn forward(ctx: ResponseCtx) -> Result<Response, ProxyError> {
    if ctx.is_sse_response {
        streaming::forward_streaming(ctx).await
    } else {
        buffered::forward_buffered(ctx).await
    }
}
