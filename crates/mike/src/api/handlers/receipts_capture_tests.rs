use super::{
    CaptureDeliveryEvidenceDto, MailboxMessageRow, MessageReceiptResponse, ReceiptConfidence,
    ReceiptEvidenceDto,
};
use chrono::Utc;
use uuid::Uuid;

fn delivered_message() -> MailboxMessageRow {
    let now = Utc::now();
    MailboxMessageRow {
        message_uuid: Uuid::new_v4(),
        to_session_uuid: Uuid::new_v4(),
        from_session_uuid: Some(Uuid::new_v4()),
        body: "peer message".to_owned(),
        queued_at: now,
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
fn capture_delivery_without_acknowledgement_preserves_read_uncertainty() -> anyhow::Result<()> {
    let exchange_uuid = Uuid::new_v4();
    let response = MessageReceiptResponse::from_evidence(
        delivered_message(),
        Vec::new(),
        vec![CaptureDeliveryEvidenceDto {
            exchange_uuid,
            delivered_at: Utc::now().to_rfc3339(),
        }],
    );
    assert_eq!(response.confidence, ReceiptConfidence::None);
    assert!(response.delivery_row.consumed_at.is_none());
    assert!(response.receipt_evidence.is_empty());
    let json = serde_json::to_value(response)?;
    assert_eq!(json["confidence"], "none");
    assert_eq!(
        json["capture_delivery_evidence"][0]["exchange_uuid"],
        exchange_uuid.to_string()
    );
    assert!(
        json["capture_delivery_evidence"][0]
            .get("request_id")
            .is_none()
    );
    assert!(json["delivery_row"].get("consumed_at").is_none());
    Ok(())
}

#[test]
fn explicit_acknowledgement_without_traffic_citations_reports_acknowledged() {
    let mut msg = delivered_message();
    msg.consumed_at = Some(Utc::now());
    let response = MessageReceiptResponse::from_evidence(msg, Vec::new(), Vec::new());
    assert_eq!(response.confidence, ReceiptConfidence::Acknowledged);
    assert!(response.delivery_row.consumed_at.is_some());
}

#[test]
fn traffic_citation_confidence_keeps_existing_match_semantics() {
    for (match_kind, expected) in [
        ("message_uuid", ReceiptConfidence::High),
        ("body_text", ReceiptConfidence::Medium),
    ] {
        let evidence = ReceiptEvidenceDto {
            request_id: 42,
            created_at: Utc::now().to_rfc3339(),
            match_kind,
            excerpt: Some("recipient citation".to_owned()),
        };
        let response =
            MessageReceiptResponse::from_evidence(delivered_message(), vec![evidence], Vec::new());
        assert_eq!(response.confidence, expected);
        assert!(response.capture_delivery_evidence.is_empty());
    }
}
