use serde_json::Value;

use super::assemble::assemble_message_json;
use super::rehydrate;
use super::{PersistError, PersistHandle};

pub struct ConversationStep {
    pub predecessor: Option<i64>,
    pub linked: bool,
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
        let linked = self.predecessor_is_linked(request_id).await?;
        let sent = rehydrate::fetch_request_sent_body_by_id(
            &self.pool,
            u64::try_from(request_id).unwrap_or_default(),
        )
        .await?;
        let body = match sent {
            Some(sent) => sent.to_vec(),
            None => row.request_body.unwrap_or_default(),
        };
        let reply = assemble_message_json(&self.pool, request_id).await?;
        Ok(Some(ConversationStep {
            predecessor: row.predecessor_request_id,
            linked,
            body,
            reply,
        }))
    }

    async fn predecessor_is_linked(&self, request_id: i64) -> Result<bool, PersistError> {
        let linked: Option<bool> = sqlx::query_scalar(PREDECESSOR_IS_LINKED) // grader-allow: literal with a bind; no .sqlx entry is prepared on infra
            .bind(request_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(linked.unwrap_or(false))
    }
}

const PREDECESSOR_IS_LINKED: &str = "SELECT pr.code IN ('thread_link', 'thread_restart')
       FROM requests.requests r
       JOIN requests.predecessor_rules pr ON pr.id = r.predecessor_rule_id
      WHERE r.id = $1";
