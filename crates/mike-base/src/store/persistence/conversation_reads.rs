use serde_json::Value;

use super::assemble::assemble_message_json;
use super::{PersistError, PersistHandle};

pub struct ConversationStep {
    pub predecessor: Option<i64>,
    pub body: Vec<u8>,
    pub reply: Option<Value>,
}

impl PersistHandle {
    pub async fn request_answered_by(
        &self,
        upstream_message_id: &str,
    ) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT rr.request_id
               FROM responses.responses rr
              WHERE rr.upstream_message_id = $1
              ORDER BY rr.request_id DESC
              LIMIT 1",
            upstream_message_id,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn conversation_step(
        &self,
        request_id: i64,
    ) -> Result<Option<ConversationStep>, PersistError> {
        let row = sqlx::query!(
            "SELECT r.predecessor_request_id, r.request_body
               FROM requests.requests r
              WHERE r.id = $1",
            request_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let reply = assemble_message_json(&self.pool, request_id).await?;
        Ok(Some(ConversationStep {
            predecessor: row.predecessor_request_id,
            body: row.request_body.unwrap_or_default(),
            reply,
        }))
    }
}
