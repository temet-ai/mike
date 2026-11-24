use super::{
    CaptureDeliveryEvidenceDto, MailboxLifecycle, mailbox_message_dto,
    mailbox_message_dto_with_activity, mailbox_status_and_staleness,
};
use crate::store::persistence::mailbox::{
    CaptureDeliveryEvidence, MailboxMessageRow, MailboxMessageWithActivity,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

fn delivered_row(now: DateTime<Utc>) -> MailboxMessageRow {
    MailboxMessageRow {
        message_uuid: Uuid::new_v4(),
        to_session_uuid: Uuid::new_v4(),
        from_session_uuid: Some(Uuid::new_v4()),
        body: "peer mail".to_owned(),
        queued_at: now - chrono::Duration::minutes(30),
        claimed_at: Some(now),
        sent_request_id: None,
        sent_at: Some(now),
        consumed_at: None,
        dead_lettered_at: None,
        channel_delivered_at: None,
        delivery_attempts: 1,
        thread_uuid: None,
        in_reply_to_uuid: None,
    }
}

#[test]
fn capture_delivery_without_claude_request_is_delivered_without_acknowledgement()
-> anyhow::Result<()> {
    let now = Utc::now();
    let dto = mailbox_message_dto(delivered_row(now), now, 10);
    assert_eq!(dto.status, "delivered");
    assert_eq!(dto.delivery_state, None);
    assert!(dto.sent_request_id.is_none());
    assert!(dto.consumed_at.is_none());
    assert!(dto.pending_minutes.is_none());
    assert!(dto.stale.is_none());
    assert!(
        serde_json::to_value(dto)?
            .get("capture_delivery_evidence")
            .is_none()
    );
    Ok(())
}

#[test]
fn capture_delivery_with_old_activity_keeps_delivered_state() {
    let now = Utc::now();
    let row = MailboxMessageWithActivity {
        row: delivered_row(now),
        recipient_last_request_at: Some(now - chrono::Duration::days(1)),
        recipient_last_eligible_turn_at: None,
        context_items: Vec::new(),
    };
    let dto = mailbox_message_dto_with_activity(row, now, 10, 60);
    assert_eq!(dto.status, "delivered");
    assert_eq!(dto.delivery_state, None);
}

#[test]
fn capture_evidence_serialization_preserves_exchange_identity_and_full_timestamp()
-> anyhow::Result<()> {
    let now = Utc::now();
    let exchange_uuid = Uuid::new_v4();
    let evidence = CaptureDeliveryEvidenceDto::from(CaptureDeliveryEvidence {
        exchange_uuid,
        delivered_at: now,
    });
    let mut dto = mailbox_message_dto(delivered_row(now), now, 10);
    dto.capture_delivery_evidence = Some(vec![evidence]);
    let json = serde_json::to_value(dto)?;
    let evidence = &json["capture_delivery_evidence"][0];
    assert_eq!(evidence["exchange_uuid"], exchange_uuid.to_string());
    assert_eq!(evidence["delivered_at"], now.to_rfc3339());
    assert!(evidence.get("request_id").is_none());
    assert!(json["consumed_at"].is_null());
    Ok(())
}

#[test]
fn capture_delivery_lifecycle_keeps_explicit_terminal_states() {
    let now = Utc::now();
    for (consumed_at, dead_lettered_at, expected) in [
        (Some(now), None, "consumed"),
        (None, Some(now), "dead_lettered"),
    ] {
        let lifecycle = MailboxLifecycle {
            consumed_at,
            dead_lettered_at,
            sent_request_id: None,
            sent_at: Some(now),
            channel_delivered_at: None,
            claimed_at: Some(now),
            queued_at: now,
        };
        let (status, _, _) = mailbox_status_and_staleness(lifecycle, now, 10);
        assert_eq!(status, expected);
    }
}
