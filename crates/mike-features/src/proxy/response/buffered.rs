use axum::body::Body;
use axum::response::Response;

use crate::error::ProxyError;

use super::ResponseCtx;

pub async fn forward_buffered(ctx: ResponseCtx) -> Result<Response, ProxyError> {
    let ResponseCtx {
        state,
        request_id,
        status,
        stored_resp_headers,
        upstream_request_id,
        upstream_resp,
        session_id,
        session_toggles,
        in_flight_guard,
        handler_start,
        resp_headers,
        context_focus_asked,
        ..
    } = ctx;

    let max_resp_body = state.config.max_response_body_bytes;
    let resp_body = {
        let stream = upstream_resp.bytes_stream();
        use futures::StreamExt;
        let mut collected = Vec::new();
        let mut stream = std::pin::pin!(stream);
        let mut overflow = false;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ProxyError::Internal(e.to_string()))?;
            if max_resp_body > 0 && collected.len() as u64 + chunk.len() as u64 > max_resp_body {
                overflow = true;
                break;
            }
            collected.extend_from_slice(&chunk);
        }
        if overflow {
            crate::observability::shed::emit(
                crate::observability::shed::MemoryShedEvent {
                    event: "memory.shed",
                    path: crate::observability::shed::ShedPath::ResponseBody,
                    reason: crate::observability::shed::ShedReason::RejectIncoming,
                    bytes: collected.len() as u64,
                    limit_bytes: max_resp_body,
                    request_id: Some(request_id.as_u64()),
                    session_id: Some(session_id.clone()),
                    note: "response_body_rejected",
                },
                Some(&state.event_tx),
            );
            return Ok(crate::proxy::error_response::response_too_large(
                max_resp_body,
            ));
        }
        bytes::Bytes::from(collected)
    };

    let resp_text = crate::extract::response::extract_accumulated_text_from_body(&resp_body);

    crate::store::RequestStore::complete_non_streaming(
        &state.store,
        crate::store::request_completion::NonStreamingCompletion {
            request_id,
            response_status: status.as_u16(),
            response_headers: stored_resp_headers,
            response_body: resp_body.clone(),
            response_text: resp_text,
            upstream_request_id,
        },
    )
    .await;
    crate::weasel_intercept::evidence::note_unscanned_buffered(
        &state,
        &session_toggles,
        status.is_success(),
        &session_id,
        request_id,
    );
    in_flight_guard.forget();
    state.in_flight.remove(request_id);

    tracing::info!(
        request_id = %request_id,
        status = status.as_u16(),
        duration_ms = handler_start.elapsed().as_millis() as u64,
        "request complete"
    );

    let client_body = context_focus_asked
        .then(|| crate::context_focus::strip::strip_json_body(&resp_body))
        .flatten();
    let mut builder = Response::builder().status(status);
    for (name, value) in resp_headers.iter() {
        if client_body.is_some() && name == axum::http::header::CONTENT_LENGTH {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from(client_body.unwrap_or(resp_body)))
        .map_err(|e| ProxyError::Internal(e.to_string()))
}
