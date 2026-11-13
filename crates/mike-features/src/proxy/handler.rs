use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::response::Response;

use crate::error::ProxyError;
use crate::proxy::engine;
use crate::state::AppState;

pub const MAX_HEADERS: usize = 64;

pub const MAX_HEADER_BYTES: usize = 8192;

#[tracing::instrument(
    name = "proxy_request",
    skip_all,
    fields(
        method = %req.method(),
        path = %req.uri().path(),
        session_id = tracing::field::Empty,
        request_id = tracing::field::Empty,
    )
)]
pub async fn proxy_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response, ProxyError> {
    let handler_start = std::time::Instant::now();

    let accepted = match engine::accept(&state, req).await? {
        engine::AcceptOutcome::EarlyResponse(response) => return Ok(response),
        engine::AcceptOutcome::Accepted(accepted) => *accepted,
    };
    tracing::Span::current().record("session_id", accepted.signals.session_id.as_str());

    let pipeline = match engine::run_request_pipeline(&state, accepted).await? {
        engine::PipelineOutcome::EarlyResponse(response) => return Ok(response),
        engine::PipelineOutcome::Continue(pipeline) => *pipeline,
    };
    tracing::Span::current().record("request_id", tracing::field::display(&pipeline.request_id));

    let forwarded = engine::forward_to_upstream(
        &state,
        pipeline.request_id,
        pipeline.parts.method.clone(),
        &pipeline.parts.uri,
        &pipeline.parts.headers,
        pipeline.upstream_body.clone(),
    )
    .await?;

    engine::finalize_and_respond(state, handler_start, pipeline, forwarded).await
}

#[cfg(test)]
#[path = "handler_tests.rs"]
pub mod tests;
