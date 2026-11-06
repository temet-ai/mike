use std::time::Instant;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Uri};
use axum::response::Response;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;

use crate::config::AppState;
use crate::error::ProxyError;
use crate::metrics::COUNTER_REMOTE_CONTROL_STREAM_EVENTS_TOTAL;

use super::route::{
    ChannelSignals, ClassifiedPayload, PayloadSessionLink, RemoteControlRoute, stored_body,
};

mod channel_queue;
mod persist;

pub(crate) use persist::CallWriters;

#[derive(Debug)]
pub(super) struct CapturedCall {
    pub(super) cse_id: String,
    pub(super) method: String,
    pub(super) path: String,
    pub(super) endpoint: &'static str,
    pub(super) http_status: i32,
    pub(super) request_body: Option<Bytes>,
    pub(super) request_body_sha256: Option<[u8; 32]>,
    pub(super) request_body_size: Option<i32>,
    pub(super) payloads: Vec<OwnedPayload>,
    pub(super) payload_link: Option<PayloadSessionLink>,
    pub(super) response_body: Option<Bytes>,
    pub(super) response_body_sha256: Option<[u8; 32]>,
    pub(super) response_body_size: Option<i32>,
    pub(super) signals: ChannelSignals,
    pub(super) created_at: DateTime<Utc>,
    pub(super) completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub(super) struct OwnedPayload {
    pub(super) ordinal: i32,
    pub(super) payload_type: String,
    pub(super) body: Option<Bytes>,
    pub(super) body_sha256: [u8; 32],
    pub(super) body_size: i32,
}

impl From<&ClassifiedPayload<'_>> for OwnedPayload {
    fn from(payload: &ClassifiedPayload<'_>) -> Self {
        Self {
            ordinal: payload.ordinal,
            payload_type: payload.payload_type.to_string(),
            body: payload.body.map(Bytes::copy_from_slice),
            body_sha256: payload.body_sha256,
            body_size: payload.body_size,
        }
    }
}

pub(crate) async fn forward_and_capture(
    state: &AppState,
    route: &RemoteControlRoute<'_>,
    method: Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, ProxyError> {
    let created_at = Utc::now();
    let started = Instant::now();
    let request_bytes = request_carried_a_body(headers).then(|| body.clone());
    let request_capture = capture_request_body(route, request_bytes.as_deref());

    let upstream_resp =
        super::super::forward::send_upstream(state, method.clone(), uri, headers, body).await?;
    let status = upstream_resp.status();
    let resp_headers = super::super::headers::filter_response_headers(upstream_resp.headers());
    let is_sse = resp_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/event-stream"));

    let call = CapturedCall {
        cse_id: route.cse_id.to_string(),
        method: method.as_str().to_string(),
        path: route.suffix.to_string(),
        endpoint: route.endpoint_label(),
        http_status: i32::from(status.as_u16()),
        request_body: request_capture.request_body,
        request_body_sha256: request_capture.request_body_sha256,
        request_body_size: request_capture.request_body_size,
        payloads: request_capture.payloads,
        payload_link: request_capture.payload_link,
        response_body: None,
        response_body_sha256: None,
        response_body_size: None,
        signals: ChannelSignals::default(),
        created_at,
        completed_at: None,
    };

    if is_sse {
        return open_stream_capture(OpenStreamCaptureArgs {
            state,
            status,
            resp_headers: &resp_headers,
            route,
            request_bytes: request_bytes.as_deref(),
            call,
            upstream_resp,
        });
    }

    finish_buffered_capture(BufferedCaptureArgs {
        state,
        method,
        status,
        resp_headers,
        upstream_resp,
        route,
        request_bytes,
        call,
        started,
    })
    .await
}

struct RequestCaptureFields {
    payloads: Vec<OwnedPayload>,
    payload_link: Option<PayloadSessionLink>,
    request_body: Option<Bytes>,
    request_body_sha256: Option<[u8; 32]>,
    request_body_size: Option<i32>,
}

fn capture_request_body(
    route: &RemoteControlRoute<'_>,
    request_bytes: Option<&[u8]>,
) -> RequestCaptureFields {
    let plan = route.plan_request(request_bytes);
    let payloads = plan.payloads.iter().map(OwnedPayload::from).collect();
    let request = stored_body(request_bytes, plan.capture);
    RequestCaptureFields {
        payloads,
        payload_link: plan.payload_link,
        request_body: request.bytes.map(Bytes::copy_from_slice),
        request_body_sha256: request.sha256,
        request_body_size: request.size,
    }
}

fn apply_payload_link(signals: &mut ChannelSignals, link: Option<PayloadSessionLink>) {
    let Some(link) = link else { return };
    signals.local_session_uuid = Some(link.session_uuid);
    signals.local_cwd = link.cwd;
    signals.client_version = link.client_version;
}

struct OpenStreamCaptureArgs<'a> {
    state: &'a AppState,
    status: reqwest::StatusCode,
    resp_headers: &'a HeaderMap,
    route: &'a RemoteControlRoute<'a>,
    request_bytes: Option<&'a [u8]>,
    call: CapturedCall,
    upstream_resp: reqwest::Response,
}

fn open_stream_capture(args: OpenStreamCaptureArgs<'_>) -> Result<Response, ProxyError> {
    let OpenStreamCaptureArgs {
        state,
        status,
        resp_headers,
        route,
        request_bytes,
        mut call,
        upstream_resp,
    } = args;
    call.signals = route.channel_signals(request_bytes, None);
    apply_payload_link(&mut call.signals, call.payload_link.take());
    tracing::info!(
        cse_id = %call.cse_id,
        path = %call.path,
        status = status.as_u16(),
        "remote control stream opened"
    );
    metrics::counter!(COUNTER_REMOTE_CONTROL_STREAM_EVENTS_TOTAL, "event" => "opened").increment(1);
    let body = stream_through(state, call, upstream_resp);
    build_response(status, resp_headers, body)
}

struct BufferedCaptureArgs<'a> {
    state: &'a AppState,
    method: Method,
    status: reqwest::StatusCode,
    resp_headers: HeaderMap,
    upstream_resp: reqwest::Response,
    route: &'a RemoteControlRoute<'a>,
    request_bytes: Option<Bytes>,
    call: CapturedCall,
    started: Instant,
}

async fn finish_buffered_capture(args: BufferedCaptureArgs<'_>) -> Result<Response, ProxyError> {
    let BufferedCaptureArgs {
        state,
        method,
        status,
        resp_headers,
        upstream_resp,
        route,
        request_bytes,
        mut call,
        started,
    } = args;

    let (client_body, collected) = read_response(state, upstream_resp).await?;
    let response_bytes = response_carries_a_body(&method, status)
        .then_some(collected)
        .flatten();
    let response = stored_body(response_bytes.as_deref(), route.response_capture());
    call.response_body = response.bytes.map(Bytes::copy_from_slice);
    call.response_body_sha256 = response.sha256;
    call.response_body_size = response.size;
    call.signals = route.channel_signals(request_bytes.as_deref(), response_bytes.as_deref());
    apply_payload_link(&mut call.signals, call.payload_link.take());
    call.completed_at = Some(Utc::now());

    tracing::debug!(
        cse_id = %call.cse_id,
        path = %call.path,
        method = %call.method,
        status = status.as_u16(),
        elapsed_ms = started.elapsed().as_millis(),
        "remote control call captured"
    );

    persist::submit(state, call, None);

    build_response(status, &resp_headers, client_body)
}

fn request_carried_a_body(headers: &HeaderMap) -> bool {
    headers.contains_key(axum::http::header::CONTENT_LENGTH)
        || headers.contains_key(axum::http::header::TRANSFER_ENCODING)
}

fn response_carries_a_body(method: &Method, status: reqwest::StatusCode) -> bool {
    method != Method::HEAD
        && !status.is_informational()
        && status.as_u16() != 204
        && status.as_u16() != 304
}

async fn read_response(
    state: &AppState,
    upstream_resp: reqwest::Response,
) -> Result<(Body, Option<Bytes>), ProxyError> {
    let max_body = state.config.max_response_body_bytes;
    let mut stream = Box::pin(upstream_resp.bytes_stream());
    let mut collected: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ProxyError::Internal(e.to_string()))?;
        if max_body > 0 && collected.len() as u64 + chunk.len() as u64 > max_body {
            tracing::warn!(
                buffered_bytes = collected.len(),
                limit_bytes = max_body,
                "remote control response exceeds the response-body cap; forwarding it whole and recording no response body"
            );
            let replayed: [reqwest::Result<Bytes>; 2] =
                [Ok(Bytes::from(std::mem::take(&mut collected))), Ok(chunk)];
            let rest = futures::stream::iter(replayed).chain(stream);
            return Ok((Body::from_stream(rest), None));
        }
        collected.extend_from_slice(&chunk);
    }
    let bytes = Bytes::from(collected);
    let client_body = Body::from_stream(futures::stream::once(std::future::ready(Ok::<
        Bytes,
        std::io::Error,
    >(
        bytes.clone()
    ))));
    Ok((client_body, Some(bytes)))
}

fn stream_through(state: &AppState, call: CapturedCall, upstream_resp: reqwest::Response) -> Body {
    let (body_tx, body_rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    let state = state.clone();
    state.clone().tracker.spawn(async move {
        let cse_id = call.cse_id.clone();
        let opens_reply_stream = call.method == "GET"
            && call.path == "/worker/events/stream"
            && (200..300).contains(&call.http_status);
        let (recorded, call_recorded) = tokio::sync::oneshot::channel();
        persist::submit(&state, call, Some(recorded));
        let call_id = call_recorded.await.ok().flatten();
        let (registration, injections) = if opens_reply_stream {
            let (registration, receiver) = state.remote_control_streams.register(cse_id.clone());
            super::repush::spawn_repush_for_stream(&state, cse_id.clone());
            (Some(registration), receiver)
        } else {
            (None, tokio::sync::mpsc::channel(1).1)
        };
        let ended = super::relay::relay(
            upstream_resp.bytes_stream(),
            body_tx,
            injections,
            state.shutdown.clone(),
        )
        .await;
        drop(registration);
        tracing::info!(%cse_id, outcome = ended.label(), "remote control stream closed");
        metrics::counter!(COUNTER_REMOTE_CONTROL_STREAM_EVENTS_TOTAL, "event" => ended.label())
            .increment(1);
        if let Some(call_id) = call_id {
            persist::close_stream_row(&state, call_id).await;
        }
    });
    Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(body_rx))
}

fn build_response(
    status: reqwest::StatusCode,
    resp_headers: &HeaderMap,
    body: Body,
) -> Result<Response, ProxyError> {
    let mut builder = Response::builder().status(status);
    for (name, value) in resp_headers.iter() {
        builder = builder.header(name, value);
    }
    builder
        .body(body)
        .map_err(|e| ProxyError::Internal(e.to_string()))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
