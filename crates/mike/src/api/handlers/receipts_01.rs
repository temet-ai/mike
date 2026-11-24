use super::*;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::mailbox::MailboxMessageRow;

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
    pub(crate) capture_delivery_evidence: Vec<CaptureDeliveryEvidenceDto>,
    pub(crate) caveat: &'static str,
}

pub(crate) const RECEIPT_CAVEAT: &str = "A delivery-row absence is not evidence of non-receipt: a session that pulls its own \
     inbox acts on a message while leaving claimed_at and consumed_at untouched. \
     confidence=none means no citation was found in the recipient's captured traffic, \
     which is not the same as the message never arriving. capture_delivery_evidence proves \
     the model endpoint accepted a request carrying this mail; it does not prove the recipient \
     read or acted on it. Codex capture delivery alone does not raise read confidence.";

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

    let evidence = fetch_traffic_receipt_evidence(handle, &msg)
        .await
        .map_err(|error| failed(&error))?;
    let capture_evidence = handle
        .launch()
        .fetch_capture_delivery_evidence(to_uuid, msg_uuid)
        .await
        .map_err(|error| failed(&error))?
        .into_iter()
        .map(Into::into)
        .collect();

    Ok(Json(MessageReceiptResponse::from_evidence(
        msg,
        evidence,
        capture_evidence,
    )))
}

async fn fetch_traffic_receipt_evidence(
    handle: &crate::store::persistence::PersistHandle,
    msg: &MailboxMessageRow,
) -> Result<Vec<ReceiptEvidenceDto>, crate::store::persistence::PersistError> {
    let uuid_hits = handle
        .launch()
        .fetch_receipt_evidence(
            msg.to_session_uuid,
            &msg.message_uuid.to_string(),
            msg.queued_at,
            RECEIPT_EVIDENCE_LIMIT,
        )
        .await?;
    let (hits, match_kind) = if uuid_hits.is_empty() {
        let needle: String = msg.body.chars().take(RECEIPT_BODY_NEEDLE_LEN).collect();
        if needle.trim().len() < 16 {
            return Ok(Vec::new());
        }
        (
            handle
                .launch()
                .fetch_receipt_evidence(
                    msg.to_session_uuid,
                    &needle,
                    msg.queued_at,
                    RECEIPT_EVIDENCE_LIMIT,
                )
                .await?,
            "body_text",
        )
    } else {
        (uuid_hits, "message_uuid")
    };
    Ok(hits
        .into_iter()
        .map(|row| ReceiptEvidenceDto {
            request_id: row.request_id,
            created_at: row.created_at.to_rfc3339(),
            match_kind,
            excerpt: row.excerpt,
        })
        .collect())
}

impl MessageReceiptResponse {
    fn from_evidence(
        msg: MailboxMessageRow,
        receipt_evidence: Vec<ReceiptEvidenceDto>,
        capture_delivery_evidence: Vec<CaptureDeliveryEvidenceDto>,
    ) -> Self {
        let confidence = if msg.consumed_at.is_some() {
            ReceiptConfidence::Acknowledged
        } else if receipt_evidence
            .iter()
            .any(|evidence| evidence.match_kind == "message_uuid")
        {
            ReceiptConfidence::High
        } else if receipt_evidence.is_empty() {
            ReceiptConfidence::None
        } else {
            ReceiptConfidence::Medium
        };
        Self {
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
            receipt_evidence,
            capture_delivery_evidence,
            caveat: RECEIPT_CAVEAT,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct LlmSpendParams {
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
}

#[cfg(test)]
#[path = "receipts_capture_tests.rs"]
mod capture_tests;
