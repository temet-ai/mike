use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

struct NothingDeliveredMemo(Memo<()>);

impl Default for NothingDeliveredMemo {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[],
            per_session: &[RowChangedTable::MailboxMessages],
        }))
    }
}

struct Delivered {
    message_uuid: uuid::Uuid,
    body: String,
}

pub fn request_carries(request_body: &[u8], text: &str) -> bool {
    let escaped = json_escaped(text);
    let needle = escaped.as_bytes();
    !needle.is_empty() && request_body.windows(needle.len()).any(|w| w == needle)
}

fn json_escaped(text: &str) -> String {
    serde_json::to_string(text)
        .ok()
        .and_then(|quoted| {
            quoted
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))
                .map(str::to_string)
        })
        .unwrap_or_else(|| text.to_string())
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn consume_mail_carried_by(
        self,
        to_session_uuid: uuid::Uuid,
        request_body: &[u8],
    ) -> Result<Vec<uuid::Uuid>, PersistError> {
        let nothing = self.memo::<NothingDeliveredMemo>();
        let memo = self.feed_memos.bound(&nothing.0);
        if memo
            .as_ref()
            .is_some_and(|memo| memo.get(to_session_uuid).is_some())
        {
            return Ok(Vec::new());
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let delivered = self.delivered_unconsumed(to_session_uuid).await?;
        if let (Some(memo), Some(ticket), true) = (memo, ticket, delivered.is_empty()) {
            memo.remember(to_session_uuid, ticket, ());
        }
        let carried: Vec<uuid::Uuid> = delivered
            .into_iter()
            .filter(|m| request_carries(request_body, &m.body))
            .map(|m| m.message_uuid)
            .collect();
        if carried.is_empty() {
            return Ok(carried);
        }
        self.consume(carried).await
    }

    async fn delivered_unconsumed(
        self,
        to_session_uuid: uuid::Uuid,
    ) -> Result<Vec<Delivered>, PersistError> {
        crate::metrics::time_db("mailbox_consume_carried", async move {
            Ok(sqlx::query_as!(
                Delivered,
                r#"SELECT m.message_uuid AS "message_uuid!", m.body AS "body!"
                     FROM mailbox.messages m
                     JOIN sessions.sessions recip ON recip.id = m.to_session_id
                    WHERE recip.session_uuid = $1
                      AND m.consumed_at IS NULL
                      AND m.dead_lettered_at IS NULL
                      AND (m.channel_delivered_at IS NOT NULL OR m.sent_request_id IS NOT NULL)"#,
                to_session_uuid,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    async fn consume(self, carried: Vec<uuid::Uuid>) -> Result<Vec<uuid::Uuid>, PersistError> {
        crate::metrics::time_db("mailbox_consume_carried", async move {
            Ok(sqlx::query_scalar!(
                r#"UPDATE mailbox.messages
                      SET consumed_at = now()
                    WHERE message_uuid = ANY($1)
                      AND consumed_at IS NULL
                RETURNING message_uuid AS "message_uuid!""#,
                &carried,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::request_carries;

    #[test]
    fn text_with_quotes_and_newlines_is_found_in_its_json_escaped_form() {
        let body = br#"{"text":"a \"quoted\" word\nsecond line"}"#;
        assert!(request_carries(body, "a \"quoted\" word\nsecond line"));
    }

    #[test]
    fn text_the_request_does_not_hold_is_not_carried() {
        assert!(!request_carries(br#"{"text":"other"}"#, "something else"));
        assert!(!request_carries(b"", "x"));
        assert!(!request_carries(b"abc", ""));
    }
}

#[cfg(test)]
#[path = "carried_memo_tests.rs"]
mod memo_tests;
