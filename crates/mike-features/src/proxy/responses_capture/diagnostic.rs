use super::record::{ExchangeRecord, Finished, request_model};
use crate::store::persistence::runtime::diagnostics::{DiagnosticExchange, EvidenceStatus};

pub fn exchange(record: &ExchangeRecord, finished: &Finished) -> DiagnosticExchange {
    let capture = record.request_body.lock().ok();
    let request = capture.as_ref().and_then(|capture| capture.body());
    let evidence_status = if !record.diagnostics_enabled {
        EvidenceStatus::NotCollected
    } else if finished.evidence_refused || capture.as_ref().is_some_and(|capture| capture.refused) {
        EvidenceStatus::BudgetRefused
    } else if !finished.evidence_complete || request.is_none() {
        EvidenceStatus::Incomplete
    } else {
        EvidenceStatus::Retained
    };
    let retained = evidence_status == EvidenceStatus::Retained;
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
        request_body: request.filter(|_| retained).map(<[u8]>::to_vec),
        forwarded_body: None,
        request_body_encoding: record.request_body_encoding.clone(),
        model: request
            .and_then(|body| request_model(body, record.request_body_encoding.as_deref()))
            .or_else(|| record.model.clone()),
        response_status: record
            .response_status
            .and_then(|status| status.try_into().ok()),
        response_headers: record.response_headers.clone(),
        response_body: finished
            .response_body
            .as_ref()
            .filter(|_| retained)
            .map(|body| body.to_vec()),
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
