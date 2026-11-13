use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;

#[derive(Debug, Serialize)]
pub(crate) struct ReceiptEvidenceDto {
    pub(crate) request_id: i64,
    pub(crate) created_at: String,
    pub(crate) match_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) excerpt: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DeliveryRowDto {
    pub(crate) queued_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) claimed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sent_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) consumed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dead_lettered_at: Option<String>,
    pub(crate) delivery_attempts: i32,
}

#[derive(Debug, Serialize)]
pub(crate) struct MessageReceiptResponse {
    pub(crate) message_uuid: String,
    pub(crate) to_session_id: String,
    pub(crate) confidence: ReceiptConfidence,
    pub(crate) delivery_row: DeliveryRowDto,
    pub(crate) receipt_evidence: Vec<ReceiptEvidenceDto>,
    pub(crate) caveat: &'static str,
}

pub(crate) const RECEIPT_CAVEAT: &str = "A delivery-row absence is not evidence of non-receipt: a session that pulls its own \
     inbox acts on a message while leaving claimed_at and consumed_at untouched. \
     confidence=none means no citation was found in the recipient's captured traffic, \
     which is not the same as the message never arriving.";

pub(crate) const RECEIPT_BODY_NEEDLE_LEN: usize = 60;
pub(crate) const RECEIPT_EVIDENCE_LIMIT: i64 = 20;

pub(crate) async fn confirm_message_receipt(
    State(state): State<AppState>,
    Path((session_id, message_uuid)): Path<(String, String)>,
) -> Result<Json<MessageReceiptResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Ok(to_uuid) = uuid::Uuid::parse_str(&session_id) else {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    };
    let Ok(msg_uuid) = uuid::Uuid::parse_str(&message_uuid) else {
        return Err((StatusCode::NOT_FOUND, "unknown message").into_response());
    };

    let failed = |e: &dyn std::fmt::Display| {
        tracing::warn!(error = %e, "confirm_message_receipt: query failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "receipt query failed").into_response()
    };

    let Some(msg) = handle
        .launch()
        .fetch_message_for_recipient(msg_uuid, to_uuid)
        .await
        .map_err(|e| failed(&e))?
    else {
        return Err((
            StatusCode::NOT_FOUND,
            "no such message addressed to that session",
        )
            .into_response());
    };

    let uuid_hits = handle
        .launch()
        .fetch_receipt_evidence(
            to_uuid,
            &msg_uuid.to_string(),
            msg.queued_at,
            RECEIPT_EVIDENCE_LIMIT,
        )
        .await
        .map_err(|e| failed(&e))?;

    let mut evidence: Vec<ReceiptEvidenceDto> = uuid_hits
        .into_iter()
        .map(|r| ReceiptEvidenceDto {
            request_id: r.request_id,
            created_at: r.created_at.to_rfc3339(),
            match_kind: "message_uuid",
            excerpt: r.excerpt,
        })
        .collect();
    let mut confidence = if evidence.is_empty() {
        ReceiptConfidence::None
    } else {
        ReceiptConfidence::High
    };

    if evidence.is_empty() {
        let needle: String = msg.body.chars().take(RECEIPT_BODY_NEEDLE_LEN).collect();
        if needle.trim().len() >= 16 {
            let body_hits = handle
                .launch()
                .fetch_receipt_evidence(to_uuid, &needle, msg.queued_at, RECEIPT_EVIDENCE_LIMIT)
                .await
                .map_err(|e| failed(&e))?;
            if !body_hits.is_empty() {
                confidence = ReceiptConfidence::Medium;
                evidence = body_hits
                    .into_iter()
                    .map(|r| ReceiptEvidenceDto {
                        request_id: r.request_id,
                        created_at: r.created_at.to_rfc3339(),
                        match_kind: "body_text",
                        excerpt: r.excerpt,
                    })
                    .collect();
            }
        }
    }

    if msg.consumed_at.is_some() {
        confidence = ReceiptConfidence::Acknowledged;
    }

    Ok(Json(MessageReceiptResponse {
        message_uuid: msg.message_uuid.to_string(),
        to_session_id: msg.to_session_uuid.to_string(),
        confidence,
        delivery_row: DeliveryRowDto {
            queued_at: msg.queued_at.to_rfc3339(),
            claimed_at: msg.claimed_at.map(|t| t.to_rfc3339()),
            sent_at: msg.sent_at.map(|t| t.to_rfc3339()),
            consumed_at: msg.consumed_at.map(|t| t.to_rfc3339()),
            dead_lettered_at: msg.dead_lettered_at.map(|t| t.to_rfc3339()),
            delivery_attempts: msg.delivery_attempts,
        },
        receipt_evidence: evidence,
        caveat: RECEIPT_CAVEAT,
    }))
}

#[derive(Debug, Default, Deserialize)]
pub struct LlmSpendParams {
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
}
