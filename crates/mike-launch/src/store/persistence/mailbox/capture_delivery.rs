use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CaptureDeliveryEvidence {
    pub exchange_uuid: Uuid,
    pub delivered_at: DateTime<Utc>,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn record_capture_delivery(
        self,
        to_session_uuid: Uuid,
        exchange_uuid: Uuid,
        message_uuids: &[Uuid],
    ) -> Result<Vec<Uuid>, PersistError> {
        if message_uuids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("mailbox_record_capture_delivery", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/mailbox/record_capture_delivery.sql",
                to_session_uuid,
                exchange_uuid,
                message_uuids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fetch_capture_delivery_evidence(
        self,
        to_session_uuid: Uuid,
        message_uuid: Uuid,
    ) -> Result<Vec<CaptureDeliveryEvidence>, PersistError> {
        crate::metrics::time_db("mailbox_capture_delivery_evidence", async move {
            Ok(sqlx::query_as!(
                CaptureDeliveryEvidence,
                "SELECT c.exchange_uuid, d.delivered_at
                   FROM mailbox.capture_deliveries d
                   JOIN runtime.capture_exchanges c ON c.id = d.capture_id
                   JOIN mailbox.messages m ON m.id = d.message_id
                   JOIN sessions.sessions s ON s.id = m.to_session_id
                  WHERE s.session_uuid = $1 AND m.message_uuid = $2
                    AND c.session_id = s.id
                  ORDER BY d.id",
                to_session_uuid,
                message_uuid,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}

#[cfg(test)]
#[path = "capture_delivery_tests.rs"]
mod tests;
