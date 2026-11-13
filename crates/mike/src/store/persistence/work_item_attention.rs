use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::PersistError;
use crate::work_items::attention::Attention;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttentionRecord {
    pub id: i64,
    pub inserted: bool,
    pub delivered: bool,
    pub delivery_attempts: i32,
    pub last_delivery_attempt_at: Option<DateTime<Utc>>,
    pub disposition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AttentionKey {
    pub session_db_id: i64,
    pub kind: String,
    pub subject: String,
}

struct RecordedRow {
    session_id: i64,
    kind: String,
    subject: String,
    id: i64,
    inserted: bool,
    delivered: bool,
    delivery_attempts: i32,
    last_delivery_attempt_at: Option<DateTime<Utc>>,
    disposition: String,
}

async fn record_attention(
    pool: &PgPool,
    items: &[(i64, &Attention)],
) -> Result<Vec<(AttentionKey, AttentionRecord)>, PersistError> {
    let sessions: Vec<i64> = items.iter().map(|(s, _)| *s).collect();
    let kinds: Vec<String> = items.iter().map(|(_, a)| a.kind.to_owned()).collect();
    let subjects: Vec<String> = items.iter().map(|(_, a)| a.subject.clone()).collect();
    let since: Vec<DateTime<Utc>> = items.iter().map(|(_, a)| a.since).collect();
    let messages: Vec<String> = items.iter().map(|(_, a)| a.message.clone()).collect();
    let rows = sqlx::query_file_as!(
        RecordedRow,
        "src/store/persistence/work_item_attention_record.sql",
        &sessions,
        &kinds,
        &subjects,
        &since,
        &messages,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                AttentionKey {
                    session_db_id: r.session_id,
                    kind: r.kind,
                    subject: r.subject,
                },
                AttentionRecord {
                    id: r.id,
                    inserted: r.inserted,
                    delivered: r.delivered,
                    delivery_attempts: r.delivery_attempts,
                    last_delivery_attempt_at: r.last_delivery_attempt_at,
                    disposition: r.disposition,
                },
            )
        })
        .collect())
}

async fn deliver_attention(
    pool: &PgPool,
    id: i64,
    message_uuid: uuid::Uuid,
    body: &str,
) -> Result<bool, PersistError> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        r#"INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
           SELECT $1, a.session_id, NULL, $2
             FROM agents.work_item_attention a
            WHERE a.id = $3
           ON CONFLICT (message_uuid) DO NOTHING"#,
        message_uuid,
        body,
        id,
    )
    .execute(&mut *tx)
    .await?;
    let marked = sqlx::query!(
        r#"UPDATE agents.work_item_attention a
              SET delivered_message_uuid = $1,
                  disposition = 'delivered',
                  disposition_reason = 'mailed',
                  disposition_at = now()
            WHERE a.id = $2
              AND EXISTS (SELECT 1 FROM mailbox.messages m WHERE m.message_uuid = $1)"#,
        message_uuid,
        id,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(marked > 0)
}

async fn note_delivery_failed(
    pool: &PgPool,
    id: i64,
    give_up_after: i32,
    failure: &str,
) -> Result<(), PersistError> {
    sqlx::query!(
        r#"UPDATE agents.work_item_attention
              SET delivery_attempts = delivery_attempts + 1,
                  last_delivery_attempt_at = now(),
                  disposition = CASE WHEN delivery_attempts + 1 >= $2
                                     THEN 'given_up' ELSE disposition END,
                  disposition_reason = CASE WHEN delivery_attempts + 1 >= $2
                                            THEN $3 ELSE disposition_reason END,
                  disposition_at = CASE WHEN delivery_attempts + 1 >= $2
                                        THEN now() ELSE disposition_at END
            WHERE id = $1 AND delivered_message_uuid IS NULL"#,
        id,
        give_up_after,
        failure,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn settle_attention(
    pool: &PgPool,
    id: i64,
    disposition: &str,
    reason: &str,
) -> Result<(), PersistError> {
    sqlx::query!(
        r#"UPDATE agents.work_item_attention
              SET disposition = $2, disposition_reason = $3, disposition_at = now()
            WHERE id = $1 AND disposition = 'pending'"#,
        id,
        disposition,
        reason,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn setting_text(pool: &PgPool, key: &str) -> Result<Option<String>, PersistError> {
    let value: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT value FROM config.settings WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(value.and_then(|v| v.as_str().map(str::to_string)))
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn record_work_item_attentions(
        self,
        items: &[(i64, &Attention)],
    ) -> Result<Vec<(AttentionKey, AttentionRecord)>, PersistError> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db(
            "record_work_item_attention",
            record_attention(&self.pool, items),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn record_work_item_attention(
        self,
        session_db_id: i64,
        item: &Attention,
    ) -> Result<Option<AttentionRecord>, PersistError> {
        let recorded = self
            .record_work_item_attentions(&[(session_db_id, item)])
            .await?;
        Ok(recorded.into_iter().next().map(|(_, record)| record))
    }

    pub(crate) async fn deliver_work_item_attention(
        self,
        id: i64,
        message_uuid: uuid::Uuid,
        body: &str,
    ) -> Result<bool, PersistError> {
        deliver_attention(&self.pool, id, message_uuid, body).await
    }

    pub(crate) async fn note_work_item_attention_delivery_failed(
        self,
        id: i64,
        give_up_after: i32,
        failure: &str,
    ) -> Result<(), PersistError> {
        note_delivery_failed(&self.pool, id, give_up_after, failure).await
    }

    pub(crate) async fn settle_work_item_attention(
        self,
        id: i64,
        disposition: &str,
        reason: &str,
    ) -> Result<(), PersistError> {
        settle_attention(&self.pool, id, disposition, reason).await
    }

    pub(crate) async fn setting_text(self, key: &str) -> Result<Option<String>, PersistError> {
        setting_text(&self.pool, key).await
    }
}
