use super::record::{ExchangeRecord, Finished};
use crate::store::persistence::runtime::diagnostics::{DiagnosticExchange, EvidenceStatus};

pub fn exchange(record: &ExchangeRecord, finished: &Finished) -> DiagnosticExchange {
    let evidence_status = EvidenceStatus::NotCollected;
    let summary = &finished.summary;
    let error = finished.error.clone().or_else(|| summary.error.clone()).or_else(|| {
        let gaps = summary.undecodable_frames + summary.oversized_frames;
        (gaps > 0).then(|| format!("{gaps} response frame(s) could not be observed; evidence availability is {evidence_status:?}"))
    });
    DiagnosticExchange {
        session_id: record.correlation.session_uuid,
        provider_session_id: record.correlation.provider_session_id.clone(),
        provider_thread_id: record.correlation.provider_thread_id.clone(),
        provider_turn_id: record.correlation.provider_turn_id.clone(),
        request_kind: record.correlation.request_kind.clone(),
        correlation: record.correlation.evidence.clone(),
        auth_route: record.auth_route.as_db_str().into(),
        transport: if record.websocket_refused {
            "websocket_refused"
        } else {
            "http"
        }
        .into(),
        method: record.method.clone(),
        path: record.path.clone(),
        upstream_url: record.upstream_url.clone(),
        request_headers: record.request_headers.clone(),
        forwarded_headers: record.forwarded_headers.clone(),
        request_body: None,
        forwarded_body: None,
        request_body_encoding: record.request_body_encoding.clone(),
        model: record.model.clone(),
        response_status: record
            .response_status
            .and_then(|status| status.try_into().ok()),
        response_headers: record.response_headers.clone(),
        response_body: None,
        upstream_request_id: record.upstream_request_id.clone(),
        response_id: summary.response_id.clone(),
        response_model: summary.model.clone(),
        response_state: summary.state.clone(),
        usage: summary.usage.clone(),
        stream_events: record
            .response_is_event_stream
            .then(|| i32::try_from(summary.events).unwrap_or(i32::MAX)),
        error,
        status: finished.status.as_db_str().into(),
        created_at: record.created_at,
        evidence_status,
    }
}
