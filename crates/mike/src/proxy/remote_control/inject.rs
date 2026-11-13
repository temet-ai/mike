use crate::store::persistence::proxy_repo::ProxyPersist as _;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use serde_json::json;
use tokio::sync::{RwLock, oneshot};
use uuid::Uuid;

use crate::injection::{InjectionGate, enqueue_if_admitted};
use crate::store::RequestStore;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::remote_control::PushSource;

use super::streams::PendingFrame;
use super::streams::ReplyStreams;

const STEERER_PUSH_ACK_GRACE: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(crate) struct PushDeps {
    pub(crate) persist: Option<PersistHandle>,
    pub(crate) remote_control_streams: ReplyStreams,
    pub(crate) gate: Arc<InjectionGate>,
    pub(crate) store: Arc<RwLock<RequestStore>>,
}

fn client_event(event_id: Uuid, text: &str) -> Bytes {
    let data = json!({
        "event_id": event_id, "event_type": "user", "sequence_num": 0,
        "payload": {"type": "user", "message": {"role": "user",
            "content": [{"type": "text", "text": text}]}}
    });
    Bytes::from(format!("event: client_event\ndata: {data}\n\n"))
}

pub(crate) async fn deliver_or_enqueue(
    deps: &PushDeps,
    recipient: Uuid,
    reminder: SystemReminder,
) -> bool {
    let session_id = recipient.to_string();
    if deps.gate.admit(&session_id, &reminder).may_wake()
        && try_push_reminder(deps, recipient, &reminder).await
    {
        return true;
    }
    let mut store = deps.store.write().await;
    enqueue_if_admitted(&deps.gate, &mut store, &session_id, reminder);
    false
}

pub(crate) async fn try_push_reminder(
    deps: &PushDeps,
    recipient: Uuid,
    reminder: &SystemReminder,
) -> bool {
    match push_reminder(deps, recipient, reminder, ChannelRule::ChannelFirst).await {
        Ok(pushed) => pushed,
        Err(error) => {
            tracing::warn!(%error, %recipient, "remote control wake failed; reminder remains queued");
            false
        }
    }
}

/// Push over Remote Control even when the recipient's mike channel is live.
/// Claude Code frames a Remote Control push as a teammate's request to act
/// on, but a channel message only as untrusted external data, so mail that
/// carries instructions the session must follow (a launch brief) goes this
/// way. See docs/INJECTION_TRUST.md.
pub(crate) async fn try_push_via_remote_control(
    deps: &PushDeps,
    recipient: Uuid,
    reminder: &SystemReminder,
) -> bool {
    match push_reminder(deps, recipient, reminder, ChannelRule::RemoteControl).await {
        Ok(pushed) => pushed,
        Err(error) => {
            tracing::warn!(%error, %recipient, "remote control push failed; reminder remains queued");
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelRule {
    /// Leave mailbox mail to the mike channel when one is live.
    ChannelFirst,
    /// Push over Remote Control regardless of the channel.
    RemoteControl,
}

pub(super) fn push_source(source: &ReminderSource) -> Option<PushSource> {
    Some(match source {
        ReminderSource::SessionMailbox { message_uuid, .. } => PushSource::Mailbox {
            message_uuid: *message_uuid,
        },
        ReminderSource::Steerer {
            intervention_uuid, ..
        } => PushSource::Steerer {
            intervention_uuid: intervention_uuid.parse().ok()?,
        },
        ReminderSource::Diagnostic => PushSource::Diagnostic,
        ReminderSource::Lesson => PushSource::Lesson,
        ReminderSource::Recall => PushSource::Recall,
        ReminderSource::SelfIdentity => return None,
        ReminderSource::WorkspaceBriefing => return None,
    })
}

async fn push_reminder(
    deps: &PushDeps,
    recipient: Uuid,
    reminder: &SystemReminder,
    rule: ChannelRule,
) -> Result<bool, PersistError> {
    let Some(source) = push_source(&reminder.source) else {
        return Ok(false);
    };
    if rule == ChannelRule::ChannelFirst
        && matches!(source, PushSource::Mailbox { .. })
        && crate::mcp_channel::listeners().is_live(recipient)
    {
        return Ok(false);
    }
    let Some(handle) = deps.persist.as_ref() else {
        return Ok(false);
    };
    let session_id = recipient.to_string();
    if !deps.gate.admit(&session_id, reminder).may_wake() {
        return Ok(not_pushed(recipient, &source, "wake_not_admitted"));
    }
    let mut channels = handle.proxy().fetch_channel_snapshots(&[recipient]).await?;
    channels.sort_by_key(|channel| std::cmp::Reverse(channel.stream_opened_at));
    let Some((channel, sender)) = channels.iter().find_map(|channel| {
        deps.remote_control_streams
            .sender(&channel.cse_id)
            .map(|sender| (channel, sender))
    }) else {
        return Ok(not_pushed(recipient, &source, "no_stream"));
    };
    let Ok(permit) = sender.try_reserve_owned() else {
        return Ok(not_pushed(recipient, &source, "queue_full"));
    };
    let event_id = Uuid::new_v4();
    let frame = client_event(event_id, &reminder.payload);
    let Some(reserved_at) = handle
        .proxy()
        .reserve_remote_control_injection(&channel.cse_id, recipient, &source, event_id, &frame)
        .await?
    else {
        return Ok(not_pushed(recipient, &source, "not_reservable"));
    };

    let (result, received) = oneshot::channel();
    permit.send(PendingFrame {
        bytes: frame,
        deadline: tokio::time::Instant::now() + Duration::from_secs(2),
        result,
    });
    deps.gate.record_wake(&session_id, reminder);
    let emitted = received.await.unwrap_or(false);
    if emitted {
        deps.gate.record_delivery(&session_id, reminder);
    }
    if let Err(error) = handle
        .proxy()
        .finish_remote_control_injection(event_id, emitted)
        .await
    {
        tracing::warn!(%error, %event_id, emitted, "failed to finish remote control injection record");
    }

    if emitted {
        arm_ack_fallback(deps, recipient, &source, event_id, reserved_at, reminder);
    }

    if !emitted {
        not_pushed(recipient, &source, "emit_timeout");
    }
    Ok(emitted)
}

fn arm_ack_fallback(
    deps: &PushDeps,
    recipient: Uuid,
    source: &PushSource,
    event_id: Uuid,
    reserved_at: chrono::DateTime<chrono::Utc>,
    reminder: &SystemReminder,
) {
    match source {
        PushSource::Steerer { intervention_uuid } => spawn_push_ack_fallback(
            deps.clone(),
            recipient,
            *intervention_uuid,
            reserved_at,
            reminder.clone(),
        ),
        PushSource::Mailbox { .. } => super::ack_fallback::spawn_mailbox_ack_fallback(
            deps.clone(),
            recipient,
            event_id,
            reminder.clone(),
        ),
        PushSource::Lesson | PushSource::Diagnostic | PushSource::Recall => {}
    }
}

fn not_pushed(recipient: Uuid, source: &PushSource, reason: &'static str) -> bool {
    tracing::info!(%recipient, source = source.kind(), reason, "remote control push not delivered; item stays queued");
    false
}

fn spawn_push_ack_fallback(
    deps: PushDeps,
    recipient: Uuid,
    intervention_uuid: Uuid,
    pushed_at: chrono::DateTime<chrono::Utc>,
    reminder: SystemReminder,
) {
    crate::observability::spans::spawn_detached(async move {
        tokio::time::sleep(STEERER_PUSH_ACK_GRACE).await;
        let Some(handle) = deps.persist.as_ref() else {
            return;
        };
        match handle
            .proxy()
            .release_stale_steerer_push(intervention_uuid, pushed_at)
            .await
        {
            Ok(true) => {
                let mut store = deps.store.write().await;
                enqueue_if_admitted(&deps.gate, &mut store, &recipient.to_string(), reminder);
                drop(store);
                tracing::info!(
                    %recipient,
                    %intervention_uuid,
                    "steerer push unacked past grace window — released and requeued for request-time delivery"
                );
            }
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(%error, %intervention_uuid, "steerer push release-if-unacked check failed");
            }
        }
    });
}

pub(super) async fn record_delivery_ack(
    handle: &crate::store::persistence::PersistHandle,
    call: &super::capture::CapturedCall,
    call_id: i64,
) {
    if call.method != "POST" || call.path != "/worker/events/delivery" {
        return;
    }
    let ids = received_event_ids(call.request_body.as_deref());
    if ids.is_empty() {
        return;
    }
    if let Err(error) = handle
        .proxy()
        .ack_remote_control_injections(&call.cse_id, &ids, call.created_at, call_id)
        .await
    {
        tracing::warn!(%error, %call_id, "failed to record remote control receipt");
    }
}

fn received_event_ids(body: Option<&[u8]>) -> Vec<Uuid> {
    let Some(body) = body.and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
    else {
        return Vec::new();
    };
    body.get("updates")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|update| update.get("status").and_then(|value| value.as_str()) == Some("received"))
        .filter_map(|update| update.get("event_id")?.as_str()?.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_escapes_text_and_has_no_sse_id() -> anyhow::Result<()> {
        let event_id = Uuid::new_v4();
        let text = "hello\n\nid: 999\n\"quoted\" 雪";
        let bytes = client_event(event_id, text);
        let frame = std::str::from_utf8(&bytes)?;
        assert!(!frame.lines().any(|line| line.starts_with("id:")));
        let data = frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .ok_or_else(|| anyhow::anyhow!("no data"))?;
        let value: serde_json::Value = serde_json::from_str(data)?;
        assert_eq!(value["payload"]["message"]["content"][0]["text"], text);
        assert_eq!(value["event_id"], event_id.to_string());
        Ok(())
    }

    #[test]
    fn only_valid_received_updates_are_receipts() {
        let id = Uuid::new_v4();
        let body = json!({"updates": [
            {"event_id": id, "status": "received"},
            {"event_id": Uuid::new_v4(), "status": "failed"},
            {"event_id": "malformed", "status": "received"}
        ]});
        assert_eq!(
            received_event_ids(Some(body.to_string().as_bytes())),
            vec![id]
        );
        assert!(received_event_ids(Some(b"not json")).is_empty());
    }

    #[test]
    fn push_source_maps_every_reminder_source() {
        assert!(matches!(
            push_source(&ReminderSource::SessionMailbox {
                message_uuid: Uuid::new_v4(),
                from_session_uuid: None,
            }),
            Some(PushSource::Mailbox { .. })
        ));
        let intervention_uuid = Uuid::new_v4();
        assert!(matches!(
            push_source(&ReminderSource::Steerer {
                kind: "verifier".to_string(),
                intervention_uuid: intervention_uuid.to_string(),
                advisory: false,
            }),
            Some(PushSource::Steerer { intervention_uuid: u }) if u == intervention_uuid
        ));
        assert!(
            push_source(&ReminderSource::Steerer {
                kind: "verifier".to_string(),
                intervention_uuid: "not-a-uuid".to_string(),
                advisory: false,
            })
            .is_none()
        );
        assert!(matches!(
            push_source(&ReminderSource::Diagnostic),
            Some(PushSource::Diagnostic)
        ));
        assert!(matches!(
            push_source(&ReminderSource::Lesson),
            Some(PushSource::Lesson)
        ));
        assert!(matches!(
            push_source(&ReminderSource::Recall),
            Some(PushSource::Recall)
        ));
        assert!(
            push_source(&ReminderSource::SelfIdentity).is_none(),
            "SelfIdentity is a request-construction-time append with no push representation"
        );
    }
}
