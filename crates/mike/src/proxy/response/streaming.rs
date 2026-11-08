use axum::body::Body;
use axum::http::HeaderMap;
use axum::response::Response;
use bytes::Bytes;
use tokio::sync::mpsc;

use crate::config::AppState;
use crate::error::ProxyError;
use crate::feature_toggles::ResolvedToggles;
use crate::store::models::{ProxyEvent, RequestId, RequestStatus};

use super::ResponseCtx;

pub(super) async fn forward_streaming(ctx: ResponseCtx) -> Result<Response, ProxyError> {
    let ResponseCtx {
        state,
        request_id,
        status,
        stored_resp_headers,
        upstream_request_id,
        session_toggles,
        upstream_resp,
        upstream_url,
        forwarded_headers_for_correction,
        upstream_body_for_correction,
        session_id,
        in_flight_guard,
        handler_start,
        streaming_model,
        resp_headers,
        project_path,
        proxy_tools_declared,
        context_focus_asked,
        ..
    } = ctx;

    mark_streaming_started_in_store(
        &state,
        request_id,
        status,
        stored_resp_headers,
        upstream_request_id,
    )
    .await;

    let weasel_armed =
        crate::weasel_intercept::evidence::weasel_armed(&session_toggles, &session_id);
    if !weasel_armed && crate::weasel_intercept::evidence::guarded_by_client(&session_id) {
        tracing::info!(
            session_id = %session_id,
            request_id = %request_id,
            reason_code = "weasel_served_by_client",
            "weasel intercept stands aside: the session's client guards the stream"
        );
    }
    let tool_loop_armed = proxy_tools_declared;
    let body_may_be_replaced = weasel_armed || tool_loop_armed;

    let body = if body_may_be_replaced {
        spawn_intercepted_stream(InterceptedStreamSetup {
            state: &state,
            request_id,
            upstream_resp,
            session_toggles: &session_toggles,
            weasel_armed,
            tool_loop_armed,
            upstream_url,
            forwarded_headers_for_correction,
            upstream_body_for_correction,
            session_id,
            project_path,
        })
    } else {
        spawn_plain_tee(&state, request_id, upstream_resp)
    };
    let body = if context_focus_asked {
        Body::from_stream(crate::context_focus::strip::strip_stream(
            body.into_data_stream(),
        ))
    } else {
        body
    };
    in_flight_guard.forget();

    let ttfb_ms = handler_start.elapsed().as_millis() as u64;
    broadcast_streaming_started(&state, request_id, status, ttfb_ms, streaming_model);

    build_streaming_response(
        status,
        &resp_headers,
        body,
        body_may_be_replaced || context_focus_asked,
    )
}

async fn mark_streaming_started_in_store(
    state: &AppState,
    request_id: RequestId,
    status: reqwest::StatusCode,
    stored_resp_headers: Vec<(String, String)>,
    upstream_request_id: Option<String>,
) {
    let mut store = state.store.write().await;
    store.update(&request_id, |req| {
        req.status = RequestStatus::Streaming;
        req.response_status = Some(status.as_u16());
        req.response_headers = Some(stored_resp_headers);
        if let Some(id) = upstream_request_id {
            req.upstream_request_id = Some(id);
        }
    });
}

struct InterceptedStreamSetup<'a> {
    state: &'a AppState,
    request_id: RequestId,
    upstream_resp: reqwest::Response,
    session_toggles: &'a ResolvedToggles,
    weasel_armed: bool,
    tool_loop_armed: bool,
    upstream_url: String,
    forwarded_headers_for_correction: HeaderMap,
    upstream_body_for_correction: Bytes,
    session_id: String,
    project_path: String,
}

fn spawn_intercepted_stream(setup: InterceptedStreamSetup<'_>) -> Body {
    let InterceptedStreamSetup {
        state,
        request_id,
        upstream_resp,
        session_toggles,
        weasel_armed,
        tool_loop_armed,
        upstream_url,
        forwarded_headers_for_correction,
        upstream_body_for_correction,
        session_id,
        project_path,
    } = setup;

    let (body_tx, body_rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    let (parser_tx, parser_rx) = mpsc::channel::<Bytes>(64);
    let (store_event_tx, store_event_rx) = mpsc::channel(512);

    crate::observability::spans::spawn_tracked(
        &state.tracker,
        crate::sse::parser::run_parser(
            parser_rx,
            request_id,
            state.event_tx.clone(),
            store_event_tx,
        ),
    );

    spawn_store_updater_for(state, request_id, store_event_rx);

    let forbidden_words = if weasel_armed {
        session_toggles.weasel_intercept_forbidden_words.clone()
    } else {
        Vec::new()
    };
    state
        .tracker
        .spawn(crate::weasel_intercept::run_gated_forwarder(
            upstream_resp,
            parser_tx,
            body_tx,
            crate::weasel_intercept::ForwarderCtx {
                state: state.clone(),
                upstream_url,
                forward_headers: forwarded_headers_for_correction,
                original_body: upstream_body_for_correction,
                forbidden_words,
                correction_prompt: session_toggles.weasel_intercept_correction_prompt.clone(),
                answers_tool_calls: tool_loop_armed,
                original_request_id: request_id,
                session_id,
                project_path,
            },
        ));

    Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(body_rx))
}

fn spawn_plain_tee(
    state: &AppState,
    request_id: RequestId,
    upstream_resp: reqwest::Response,
) -> Body {
    let (body, parser_rx) = crate::sse::tee::tee_sse_response(upstream_resp, &state.tracker);

    let (store_event_tx, store_event_rx) = mpsc::channel(512);

    crate::observability::spans::spawn_tracked(
        &state.tracker,
        crate::sse::parser::run_parser(
            parser_rx,
            request_id,
            state.event_tx.clone(),
            store_event_tx,
        ),
    );

    spawn_store_updater_for(state, request_id, store_event_rx);
    body
}

fn spawn_store_updater_for(
    state: &AppState,
    request_id: RequestId,
    event_rx: mpsc::Receiver<ProxyEvent>,
) {
    crate::store::updater::spawn_store_updater(
        crate::store::updater::StoreUpdaterSpawnArgs {
            store: state.store.clone(),
            event_rx,
            request_id,
            in_flight: state.in_flight.clone(),
            persist: state.persist().cloned(),
            harvester_cfg: std::sync::Arc::new(crate::memory_harvester::HarvesterConfig {
                enabled: state.feature_toggle_overlay.load().harvester,
                ..state.config.harvester.clone()
            }),
        },
        &state.tracker,
    );
}

fn broadcast_streaming_started(
    state: &AppState,
    request_id: RequestId,
    status: reqwest::StatusCode,
    ttfb_ms: u64,
    streaming_model: Option<String>,
) {
    let _ = state.event_tx.send(ProxyEvent::StreamingStarted {
        id: request_id,
        status: status.as_u16(),
        ttfb_ms,
        model: streaming_model,
    });

    tracing::debug!(
        request_id = %request_id,
        status = status.as_u16(),
        ttfb_ms = ttfb_ms,
        "streaming response started"
    );
}

fn build_streaming_response(
    status: reqwest::StatusCode,
    resp_headers: &HeaderMap,
    body: Body,
    body_may_be_replaced: bool,
) -> Result<Response, ProxyError> {
    let mut builder = Response::builder().status(status);
    for (name, value) in resp_headers.iter() {
        if body_may_be_replaced && name == axum::http::header::CONTENT_LENGTH {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
        .body(body)
        .map_err(|e| ProxyError::Internal(e.to_string()))
}
