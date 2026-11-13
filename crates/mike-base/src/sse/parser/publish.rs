use std::time::Instant;

use tokio::sync::{broadcast, mpsc};

use crate::observability::shed::{MemoryShedEvent, ShedPath, ShedReason, emit as emit_shed};
use crate::store::models::{ProxyEvent, RequestId};

const STORE_EVENT_SHED_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(1);

pub struct PublishCtx<'a> {
    pub event_tx: &'a broadcast::Sender<ProxyEvent>,
    pub store_tx: &'a mpsc::Sender<ProxyEvent>,
    pub request_id: RequestId,
    pub last_shed_at: &'a mut Option<Instant>,
}

pub async fn publish(ctx: &mut PublishCtx<'_>, event: ProxyEvent) {
    let _ = ctx.event_tx.send(event.clone());

    let send_started = Instant::now();
    let pending_event = match ctx.store_tx.try_send(event) {
        Ok(()) => {
            record_send_latency(send_started);
            return;
        }
        Err(mpsc::error::TrySendError::Full(ev)) => {
            record_backpressure_and_maybe_shed(ctx, send_started);
            ev
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            log_store_channel_closed();
            return;
        }
    };

    if ctx.store_tx.send(pending_event).await.is_err() {
        log_store_channel_closed();
    }
    record_send_latency(send_started);
}

fn log_store_channel_closed() {
    tracing::debug!("store updater channel closed, dropping event");
}

fn record_send_latency(send_started: Instant) {
    metrics::histogram!(crate::metrics::HISTOGRAM_STORE_EVENT_SEND_SECONDS)
        .record(send_started.elapsed().as_secs_f64());
}

fn record_backpressure_and_maybe_shed(ctx: &mut PublishCtx<'_>, send_started: Instant) {
    metrics::counter!(crate::metrics::COUNTER_STORE_EVENT_BACKPRESSURED_TOTAL).increment(1);

    let should_emit = ctx
        .last_shed_at
        .map(|prev| send_started.duration_since(prev) >= STORE_EVENT_SHED_DEBOUNCE)
        .unwrap_or(true);
    if !should_emit {
        return;
    }
    emit_shed(
        MemoryShedEvent {
            event: "memory.shed",
            path: ShedPath::StoreEventMpsc,
            reason: ShedReason::SoftWatermark,
            bytes: 0,
            limit_bytes: ctx.store_tx.max_capacity() as u64,
            request_id: Some(ctx.request_id.as_u64()),
            session_id: None,
            note: "store_event_backpressured",
        },
        Some(ctx.event_tx),
    );
    *ctx.last_shed_at = Some(send_started);
}
