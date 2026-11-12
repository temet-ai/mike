use chrono::Utc;
use tokio::sync::oneshot;

use crate::metrics::COUNTER_REMOTE_CONTROL_CALLS_TOTAL;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::remote_control::{
    RemoteControlCall, RemoteControlPayload, RemoteControlSignals,
};

use super::CapturedCall;
use super::channel_queue::ChannelQueues;

const FLAP_HISTORY_LIMIT: i64 = 5;
const FLAP_TRANSITION_THRESHOLD: usize = 3;
const FLAP_WINDOW_SECONDS: i64 = 60;

pub(crate) type CallWriters = ChannelQueues<QueuedCall>;

pub(crate) struct QueuedCall {
    call: CapturedCall,
    recorded: Option<oneshot::Sender<Option<i64>>>,
}

pub(super) fn submit(
    state: &AppState,
    call: CapturedCall,
    recorded: Option<oneshot::Sender<Option<i64>>>,
) {
    let Some(persist) = state.persist.as_ref().map(PersistHandle::capture) else {
        count_call(call.endpoint, "not_persisted");
        if let Some(recorded) = recorded {
            let _ = recorded.send(None);
        }
        return;
    };
    let channel = call.cse_id.clone();
    let writers = &state.remote_control_writers;
    if writers.push(&channel, QueuedCall { call, recorded }) {
        let drain = writers.clone().drain(channel, move |batch| {
            let persist = persist.clone();
            async move { record_batch(&persist, batch).await }
        });
        crate::observability::spans::spawn_tracked(&state.tracker, drain);
    }
}

fn payload_rows(call: &CapturedCall) -> Vec<RemoteControlPayload<'_>> {
    call.payloads
        .iter()
        .map(|p| RemoteControlPayload {
            ordinal: p.ordinal,
            payload_type: &p.payload_type,
            body: p.body.as_deref(),
            body_sha256: p.body_sha256,
            body_size: p.body_size,
        })
        .collect()
}

fn preserved_ids(call: &CapturedCall) -> Vec<&str> {
    call.signals
        .preserved_event_ids
        .iter()
        .flatten()
        .map(String::as_str)
        .collect()
}

fn insert_row<'a>(
    call: &'a CapturedCall,
    payloads: &'a [RemoteControlPayload<'a>],
    preserved: &'a [&'a str],
) -> RemoteControlCall<'a> {
    RemoteControlCall {
        cse_id: &call.cse_id,
        method: &call.method,
        path: &call.path,
        http_status: Some(call.http_status),
        request_body: call.request_body.as_deref(),
        request_body_sha256: call.request_body_sha256,
        request_body_size: call.request_body_size,
        response_body: call.response_body.as_deref(),
        response_body_sha256: call.response_body_sha256,
        response_body_size: call.response_body_size,
        created_at: call.created_at,
        completed_at: call.completed_at,
        signals: RemoteControlSignals {
            client_session_id: call.signals.client_session_id.as_deref(),
            worker_epoch: call.signals.worker_epoch,
            worker_status: call.signals.worker_status.as_deref(),
            idle_seconds: call.signals.idle_seconds,
            current_interval_seconds: call.signals.current_interval_seconds,
            is_compaction: call.signals.is_compaction,
            session_agent_id: call.signals.session_agent_id.as_deref(),
            preserved_event_ids: call
                .signals
                .preserved_event_ids
                .is_some()
                .then_some(preserved),
            post_turn_summary: call.signals.post_turn_summary.as_ref(),
            local_session_uuid: call.signals.local_session_uuid.as_deref(),
            local_cwd: call.signals.local_cwd.as_deref(),
            client_version: call.signals.client_version.as_deref(),
        },
        payloads,
    }
}

async fn record_batch(persist: &PersistHandle, batch: Vec<QueuedCall>) {
    let payloads: Vec<Vec<RemoteControlPayload<'_>>> =
        batch.iter().map(|q| payload_rows(&q.call)).collect();
    let preserved: Vec<Vec<&str>> = batch.iter().map(|q| preserved_ids(&q.call)).collect();
    let rows: Vec<RemoteControlCall<'_>> = batch
        .iter()
        .zip(&payloads)
        .zip(&preserved)
        .map(|((q, payloads), preserved)| insert_row(&q.call, payloads, preserved))
        .collect();
    let recorded = persist.record_remote_control_calls(&rows).await;
    drop(rows);
    match recorded {
        Ok(ids) => finish_recorded(persist, batch, ids).await,
        Err(err) => finish_failed(batch, &err),
    }
}

async fn finish_recorded(persist: &PersistHandle, batch: Vec<QueuedCall>, ids: Vec<i64>) {
    let last_status_call = batch
        .iter()
        .rposition(|q| q.call.signals.worker_status.is_some());
    for (q, id) in batch.iter().zip(&ids) {
        count_call(q.call.endpoint, "recorded");
        super::super::inject::record_delivery_ack(persist, &q.call, *id).await;
    }
    if let Some(q) = last_status_call.and_then(|index| batch.get(index)) {
        warn_on_flapping_status(persist, &q.call).await;
    }
    for (q, id) in batch.into_iter().zip(ids) {
        if let Some(recorded) = q.recorded {
            let _ = recorded.send(Some(id));
        }
    }
}

fn finish_failed(batch: Vec<QueuedCall>, err: &crate::store::persistence::PersistError) {
    for q in batch {
        tracing::warn!(
            cse_id = %q.call.cse_id,
            path = %q.call.path,
            error = %err,
            "failed to record a remote control call"
        );
        count_call(q.call.endpoint, "error");
        if let Some(recorded) = q.recorded {
            let _ = recorded.send(None);
        }
    }
}

async fn warn_on_flapping_status(persist: &PersistHandle, call: &CapturedCall) {
    if call.signals.worker_status.is_none() {
        return;
    }
    let Some(session_uuid) = call
        .signals
        .local_session_uuid
        .as_deref()
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
    else {
        return;
    };
    let Ok(history) = persist
        .fetch_worker_status_history(session_uuid, FLAP_HISTORY_LIMIT)
        .await
    else {
        return;
    };
    if history.len() < FLAP_TRANSITION_THRESHOLD {
        return;
    }
    let newest = &history[0];
    let boundary = history[FLAP_TRANSITION_THRESHOLD - 1].observed_at;
    if (newest.observed_at - boundary).num_seconds() <= FLAP_WINDOW_SECONDS {
        tracing::warn!(
            session_id = %session_uuid,
            cse_id = %call.cse_id,
            channel_id = newest.channel_id,
            from_status = newest.from_status.as_deref().unwrap_or("<none>"),
            to_status = %newest.to_status,
            call_id = ?newest.call_id,
            transitions_in_window = FLAP_TRANSITION_THRESHOLD,
            window_seconds = FLAP_WINDOW_SECONDS,
            "remote control worker_status is flapping"
        );
    }
}

pub(super) async fn close_stream_row(state: &AppState, call_id: i64) {
    let Some(persist) = &state.persist else {
        return;
    };
    if let Err(err) = persist
        .close_remote_control_stream(call_id, Utc::now())
        .await
    {
        tracing::warn!(
            call_id,
            error = %err,
            "failed to record the close of a remote control stream"
        );
    }
}

fn count_call(endpoint: &'static str, outcome: &'static str) {
    metrics::counter!(
        COUNTER_REMOTE_CONTROL_CALLS_TOTAL,
        "endpoint" => endpoint,
        "outcome" => outcome,
    )
    .increment(1);
}
