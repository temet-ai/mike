mod record;
mod tee;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Uri};
use axum::response::Response;
use bytes::Bytes;

use crate::config::AppState;
use crate::error::ProxyError;
use crate::metrics::COUNTER_SESSIONLESS_REQUESTS_TOTAL;

use record::{Finished, Recorder, SessionlessRecord};
use tee::Tee;

pub(crate) async fn forward_and_capture(
    state: &AppState,
    reason: &'static str,
    method: Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, ProxyError> {
    metrics::counter!(COUNTER_SESSIONLESS_REQUESTS_TOTAL, "reason" => reason).increment(1);
    let recorder = Recorder::from_state(state);
    let mut record = SessionlessRecord::new(reason, &method, uri, headers, body.clone());
    let upstream = match super::forward::send_upstream(state, method, uri, headers, body).await {
        Ok(upstream) => upstream,
        Err(err) => {
            recorder.record(record, Finished::failed(err.to_string()));
            return Err(err);
        }
    };
    let status = upstream.status();
    let resp_headers = crate::extract::headers::filter_response_headers(upstream.headers());
    record.set_response(status.as_u16(), upstream.headers());
    let cap = state.config.max_response_body_bytes;
    let tee = Tee::new(Box::pin(upstream.bytes_stream()), cap, recorder, record);
    let mut builder = Response::builder().status(status);
    for (name, value) in resp_headers.iter() {
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from_stream(tee))
        .map_err(|e| ProxyError::Internal(e.to_string()))
}

#[cfg(test)]
#[path = "tee_tests.rs"]
mod tee_tests;

#[cfg(test)]
#[path = "db_tests.rs"]
mod db_tests;
