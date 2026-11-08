use std::time::Instant;

use axum::response::Response;
use serde_json::Value;

use crate::compose_prompt;
use crate::config::AppState;
use crate::error::ProxyError;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};

use super::{ForwardOutcome, PipelineResult};

pub(crate) async fn finalize_and_respond(
    state: AppState,
    handler_start: Instant,
    pipeline: PipelineResult,
    forwarded: ForwardOutcome,
) -> Result<Response, ProxyError> {
    let PipelineResult {
        request_id,
        session_id,
        session_toggles,
        in_flight_guard,
        streaming_model,
        injected_reminders,
        observed_composition_applied,
        composed_system_text,
        project_path,
        carrier_eligible,
        proxy_tools_declared,
        context_focus_asked,
        ..
    } = pipeline;
    let ForwardOutcome {
        upstream_resp,
        status,
        resp_headers,
        is_sse_response,
        upstream_url,
        forwarded_headers_for_correction,
        upstream_body_for_correction,
    } = forwarded;

    if !injected_reminders.is_empty() {
        confirm_reminder_deliveries(
            &state,
            request_id,
            &session_id,
            status,
            upstream_body_for_correction.as_ref(),
            injected_reminders,
        )
        .await;
    }

    if carrier_eligible && status.is_success() {
        spawn_push_carrier_stamp(&state, &session_id, request_id, handler_start);
    }

    confirm_composition_alteration(
        &state,
        &session_id,
        observed_composition_applied,
        composed_system_text.as_deref(),
        status,
        upstream_body_for_correction.as_ref(),
    )
    .await;

    let stored_resp_headers: Vec<(String, String)> =
        super::super::headers::captured_header_pairs(&resp_headers);

    let upstream_request_id: Option<String> = resp_headers
        .get("request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(String::from);

    super::super::response::forward(super::super::response::ResponseCtx {
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
        is_sse_response,
        project_path,
        proxy_tools_declared,
        context_focus_asked,
    })
    .await
}

fn spawn_push_carrier_stamp(
    state: &AppState,
    session_id: &str,
    request_id: RequestId,
    handler_start: Instant,
) {
    let Some(persist) = state.persist.clone() else {
        return;
    };
    let Ok(session_uuid) = session_id.parse::<uuid::Uuid>() else {
        return;
    };
    let started_at = chrono::Utc::now()
        - chrono::Duration::from_std(handler_start.elapsed()).unwrap_or_default();
    let db_request_id = request_id.as_u64() as i64;
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(error) = persist
            .stamp_push_carriers(session_uuid, db_request_id, started_at)
            .await
        {
            tracing::warn!(%error, session_id = %session_uuid, db_request_id, "failed to stamp the request that carried pushed items");
        }
    });
}

async fn confirm_reminder_deliveries(
    state: &AppState,
    request_id: RequestId,
    session_id: &str,
    status: reqwest::StatusCode,
    forwarded_bytes: &[u8],
    injected_reminders: Vec<SystemReminder>,
) {
    let db_request_id = request_id.as_u64() as i64;
    for reminder in injected_reminders {
        let present = crate::reminders::appender::payload_present_in_last_user_message(
            forwarded_bytes,
            &reminder.payload,
        );
        let delivered = status.is_success() && present;

        if delivered {
            confirm_delivered_reminder(state, request_id, db_request_id, &reminder).await;
        } else {
            log_undelivered_reminder(request_id, session_id, status, &reminder);
            state
                .store
                .write()
                .await
                .requeue_reminder(session_id, reminder);
        }
    }
}

async fn confirm_delivered_reminder(
    state: &AppState,
    request_id: RequestId,
    db_request_id: i64,
    reminder: &SystemReminder,
) {
    let ReminderSource::Steerer {
        kind,
        intervention_uuid,
        ..
    } = &reminder.source
    else {
        return;
    };
    let Some(persist) = &state.persist else {
        return;
    };
    let result = persist
        .steering_backfill_delivered_request_id(intervention_uuid, db_request_id)
        .await;
    log_delivery_confirm_result(request_id, kind, intervention_uuid, result);
}

fn log_delivery_confirm_result(
    request_id: RequestId,
    kind: &str,
    intervention_uuid: &str,
    result: Result<bool, crate::store::persistence::PersistError>,
) {
    match result {
        Ok(true) => log_delivery_confirmed(request_id, kind, intervention_uuid),
        Ok(false) => log_delivery_confirm_no_match(request_id, kind, intervention_uuid),
        Err(e) => log_delivery_confirm_failed(request_id, kind, intervention_uuid, e),
    }
}

fn log_delivery_confirmed(request_id: RequestId, kind: &str, intervention_uuid: &str) {
    tracing::debug!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        "steering reminder delivery confirmed (payload verified in forwarded body)"
    );
}

fn log_delivery_confirm_no_match(request_id: RequestId, kind: &str, intervention_uuid: &str) {
    tracing::trace!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        "steering reminder delivery confirm: no matching intervention row"
    );
}

fn log_delivery_confirm_failed(
    request_id: RequestId,
    kind: &str,
    intervention_uuid: &str,
    e: crate::store::persistence::PersistError,
) {
    tracing::warn!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        error = %e,
        "steering reminder delivery confirm failed"
    );
}

fn log_undelivered_reminder(
    request_id: RequestId,
    session_id: &str,
    status: reqwest::StatusCode,
    reminder: &SystemReminder,
) {
    if status.is_success() {
        log_reminder_claimed_but_absent(request_id, session_id, reminder);
    } else {
        log_reminder_upstream_rejected(request_id, session_id, status, reminder);
    }
}

fn log_reminder_claimed_but_absent(
    request_id: RequestId,
    session_id: &str,
    reminder: &SystemReminder,
) {
    tracing::warn!(
        request_id = %request_id,
        session_id = %session_id,
        source = ?reminder.source,
        "steerer reminder inject reported success but payload absent from forwarded body — requeueing"
    );
}

fn log_reminder_upstream_rejected(
    request_id: RequestId,
    session_id: &str,
    status: reqwest::StatusCode,
    reminder: &SystemReminder,
) {
    tracing::debug!(
        request_id = %request_id,
        session_id = %session_id,
        source = ?reminder.source,
        status = status.as_u16(),
        "steerer reminder delivery unconfirmed (upstream non-2xx) — requeueing"
    );
}

async fn confirm_composition_alteration(
    state: &AppState,
    session_id: &str,
    observed_composition_applied: bool,
    composed_system_text: Option<&str>,
    status: reqwest::StatusCode,
    forwarded_bytes: &[u8],
) {
    if !observed_composition_applied {
        return;
    }
    let Some(expected_text) = composed_system_text else {
        return;
    };
    let forwarded_matches = serde_json::from_slice::<Value>(forwarded_bytes)
        .ok()
        .and_then(|v| compose_prompt::stage::system_block_text(&v, 2))
        .is_some_and(|actual| actual == expected_text);
    if !forwarded_matches || !status.is_success() {
        return;
    }
    let Some(persist) = &state.persist else {
        return;
    };
    if let Err(e) = persist
        .mark_session_prompt_alteration_used(session_id)
        .await
    {
        tracing::warn!(
            session_id = %session_id,
            error = %e,
            "ledger main-row alteration_used stamp failed"
        );
    }
}
