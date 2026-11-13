use serde::Serialize;
use tokio::sync::broadcast;

use crate::metrics::COUNTER_MEMORY_SHED_TOTAL;
use crate::store::models::ProxyEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShedPath {
    BroadcastLag,
    ResponseBody,
    HeaderCapture,
    StoreEventMpsc,
    StoreInsert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShedReason {
    RejectIncoming,
    EvictExisting,
    SoftWatermark,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryShedEvent {
    pub event: &'static str,
    pub path: ShedPath,
    pub reason: ShedReason,
    pub bytes: u64,
    pub limit_bytes: u64,
    pub request_id: Option<u64>,
    pub session_id: Option<String>,
    pub note: &'static str,
}

pub fn emit(event: MemoryShedEvent, event_tx: Option<&broadcast::Sender<ProxyEvent>>) {
    log_shed_event(&event);
    record_shed_metric(&event);
    broadcast_shed_event(event, event_tx);
}

fn log_shed_event(event: &MemoryShedEvent) {
    tracing::warn!(
        target: "mike::memory_shed",
        event = event.event,
        path = path_label(event.path),
        reason = reason_label(event.reason),
        bytes = event.bytes,
        limit_bytes = event.limit_bytes,
        request_id = event.request_id,
        session_id = event.session_id.as_deref(),
        note = event.note,
        "memory.shed"
    );
}

fn record_shed_metric(event: &MemoryShedEvent) {
    metrics::counter!(
        COUNTER_MEMORY_SHED_TOTAL,
        "path" => path_label(event.path),
        "reason" => reason_label(event.reason),
    )
    .increment(1);
}

fn broadcast_shed_event(event: MemoryShedEvent, event_tx: Option<&broadcast::Sender<ProxyEvent>>) {
    let Some(tx) = event_tx else {
        return;
    };
    if let Err(err) = tx.send(ProxyEvent::MemoryShed(event)) {
        tracing::debug!(
            target: "mike::memory_shed",
            error = %err,
            "memory.shed broadcast send failed (no live subscribers)"
        );
    }
}

pub fn path_label(path: ShedPath) -> &'static str {
    match path {
        ShedPath::BroadcastLag => "broadcast_lag",
        ShedPath::ResponseBody => "response_body",
        ShedPath::HeaderCapture => "header_capture",
        ShedPath::StoreEventMpsc => "store_event_mpsc",
        ShedPath::StoreInsert => "store_insert",
    }
}

pub fn reason_label(reason: ShedReason) -> &'static str {
    match reason {
        ShedReason::RejectIncoming => "reject_incoming",
        ShedReason::EvictExisting => "evict_existing",
        ShedReason::SoftWatermark => "soft_watermark",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{MEMORY_SHED_PATHS, MEMORY_SHED_REASONS};

    #[test]
    fn path_label_round_trips_to_metrics_constants() {
        for path in [
            ShedPath::BroadcastLag,
            ShedPath::ResponseBody,
            ShedPath::HeaderCapture,
            ShedPath::StoreEventMpsc,
            ShedPath::StoreInsert,
        ] {
            let label = path_label(path);
            assert!(
                MEMORY_SHED_PATHS.contains(&label),
                "path label {label:?} for variant {path:?} must appear in MEMORY_SHED_PATHS"
            );
        }
    }

    #[test]
    fn reason_label_round_trips_to_metrics_constants() {
        for reason in [
            ShedReason::RejectIncoming,
            ShedReason::EvictExisting,
            ShedReason::SoftWatermark,
        ] {
            let label = reason_label(reason);
            assert!(
                MEMORY_SHED_REASONS.contains(&label),
                "reason label {label:?} for variant {reason:?} must appear in MEMORY_SHED_REASONS"
            );
        }
    }

    #[test]
    fn shed_paths_match_metrics_paths_cardinality() {
        let enum_labels: std::collections::HashSet<&'static str> = [
            ShedPath::BroadcastLag,
            ShedPath::ResponseBody,
            ShedPath::HeaderCapture,
            ShedPath::StoreEventMpsc,
            ShedPath::StoreInsert,
        ]
        .into_iter()
        .map(path_label)
        .collect();
        let metrics_labels: std::collections::HashSet<&'static str> =
            MEMORY_SHED_PATHS.iter().copied().collect();
        assert_eq!(
            enum_labels, metrics_labels,
            "ShedPath variants and MEMORY_SHED_PATHS must enumerate the same label set"
        );
    }

    #[test]
    fn emit_without_broadcast_subscriber_does_not_panic() {
        let event = MemoryShedEvent {
            event: "memory.shed",
            path: ShedPath::HeaderCapture,
            reason: ShedReason::RejectIncoming,
            bytes: 1024,
            limit_bytes: 512,
            request_id: Some(42),
            session_id: Some("session-test".to_string()),
            note: "header_capture_clipped",
        };
        emit(event, None);
    }

    #[tokio::test]
    async fn emit_delivers_memory_shed_frame_to_active_subscriber() -> anyhow::Result<()> {
        let (tx, mut rx) = broadcast::channel::<ProxyEvent>(8);
        let event = MemoryShedEvent {
            event: "memory.shed",
            path: ShedPath::ResponseBody,
            reason: ShedReason::RejectIncoming,
            bytes: 65536,
            limit_bytes: 32768,
            request_id: Some(7),
            session_id: Some("session-r".to_string()),
            note: "response_body_rejected",
        };
        emit(event, Some(&tx));
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("broadcast must deliver within 1s"))?
            .map_err(|e| anyhow::anyhow!("recv must succeed for an active subscriber: {e}"))?;
        match frame {
            ProxyEvent::MemoryShed(received) => {
                assert_eq!(received.event, "memory.shed");
                assert_eq!(received.path, ShedPath::ResponseBody);
                assert_eq!(received.reason, ShedReason::RejectIncoming);
                assert_eq!(received.bytes, 65536);
                assert_eq!(received.limit_bytes, 32768);
                assert_eq!(received.request_id, Some(7));
                assert_eq!(received.session_id.as_deref(), Some("session-r"));
                assert_eq!(received.note, "response_body_rejected");
            }
            other => anyhow::bail!("expected ProxyEvent::MemoryShed, got {other:?}"),
        }
        Ok(())
    }
}
