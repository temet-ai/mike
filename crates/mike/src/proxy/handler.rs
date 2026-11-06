use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::response::Response;

use crate::config::AppState;
use crate::error::ProxyError;
use crate::proxy::engine;

pub(crate) const MAX_HEADERS: usize = 64;

pub(crate) const MAX_HEADER_BYTES: usize = 8192;

pub(crate) async fn proxy_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response, ProxyError> {
    let handler_start = std::time::Instant::now();

    let accepted = match engine::accept(&state, req).await? {
        engine::AcceptOutcome::EarlyResponse(response) => return Ok(response),
        engine::AcceptOutcome::Accepted(accepted) => *accepted,
    };

    let pipeline = match engine::run_request_pipeline(&state, accepted).await? {
        engine::PipelineOutcome::EarlyResponse(response) => return Ok(response),
        engine::PipelineOutcome::Continue(pipeline) => *pipeline,
    };

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
mod tests;
