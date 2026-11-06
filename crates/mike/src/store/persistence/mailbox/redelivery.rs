use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

use super::types::{MailboxMessageRow, ReapOutcome, ReloadedMailboxRow};

impl PersistHandle {
    pub async fn fetch_pending_redelivery(
        &self,
        to_session_uuid: uuid::Uuid,
        limit: i64,
    ) -> Result<Vec<ReloadedMailboxRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_pending_redelivery", async move {
            let rows = sqlx::query!(
                r#"SELECT
                       m.message_uuid      AS "message_uuid!",
                       recip.session_uuid  AS "to_session_uuid!",
                       sender.session_uuid AS "from_session_uuid?",
                       m.body              AS "body!",
                       m.queued_at         AS "queued_at!",
                       m.claimed_at        AS "claimed_at?",
                       m.sent_request_id   AS "sent_request_id?",
                       m.sent_at           AS "sent_at?",
                       m.consumed_at       AS "consumed_at?",
                       m.dead_lettered_at  AS "dead_lettered_at?",
                       m.delivery_attempts AS "delivery_attempts!",
                       m.id = (
                           SELECT MIN(p.id) FROM mailbox.messages p
                            WHERE p.to_session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)
                       ) AS "first_for_recipient!"
                   FROM mailbox.messages m
                   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
                   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
                  WHERE recip.session_uuid = $1
                    AND m.sent_at IS NOT NULL
                    AND m.consumed_at IS NULL
                    AND m.dead_lettered_at IS NULL
                  ORDER BY m.id ASC
                  LIMIT $2"#,
                to_session_uuid,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ReloadedMailboxRow {
                    row: MailboxMessageRow {
                        message_uuid: r.message_uuid,
                        to_session_uuid: r.to_session_uuid,
                        from_session_uuid: r.from_session_uuid,
                        body: r.body,
                        queued_at: r.queued_at,
                        claimed_at: r.claimed_at,
                        sent_request_id: r.sent_request_id,
                        sent_at: r.sent_at,
                        consumed_at: r.consumed_at,
                        dead_lettered_at: r.dead_lettered_at,
                        channel_delivered_at: None,
                        delivery_attempts: r.delivery_attempts,
                        thread_uuid: None,
                        in_reply_to_uuid: None,
                    },
                    first_for_recipient: r.first_for_recipient,
                })
                .collect())
        })
        .await
    }

    pub(crate) async fn fetch_all_pending_redelivery(
        &self,
        limit: i64,
    ) -> Result<Vec<ReloadedMailboxRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_all_pending_redelivery", async move {
            let rows = sqlx::query!(
                r#"WITH first_ids AS (
                       SELECT to_session_id, MIN(id) AS first_id
                         FROM mailbox.messages
                        GROUP BY to_session_id
                   )
                   SELECT
                       m.message_uuid      AS "message_uuid!",
                       recip.session_uuid  AS "to_session_uuid!",
                       sender.session_uuid AS "from_session_uuid?",
                       m.body              AS "body!",
                       m.queued_at         AS "queued_at!",
                       m.claimed_at        AS "claimed_at?",
                       m.sent_request_id   AS "sent_request_id?",
                       m.sent_at           AS "sent_at?",
                       m.consumed_at       AS "consumed_at?",
                       m.dead_lettered_at  AS "dead_lettered_at?",
                       m.delivery_attempts AS "delivery_attempts!",
                       fi.first_id = m.id AS "first_for_recipient!"
                   FROM mailbox.messages m
                   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
                   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
                   JOIN first_ids fi ON fi.to_session_id = m.to_session_id
                  WHERE m.sent_at IS NOT NULL
                    AND m.consumed_at IS NULL
                    AND m.dead_lettered_at IS NULL
                  ORDER BY m.id ASC
                  LIMIT $1"#,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| ReloadedMailboxRow {
                    row: MailboxMessageRow {
                        message_uuid: r.message_uuid,
                        to_session_uuid: r.to_session_uuid,
                        from_session_uuid: r.from_session_uuid,
                        body: r.body,
                        queued_at: r.queued_at,
                        claimed_at: r.claimed_at,
                        sent_request_id: r.sent_request_id,
                        sent_at: r.sent_at,
                        consumed_at: r.consumed_at,
                        dead_lettered_at: r.dead_lettered_at,
                        channel_delivered_at: None,
                        delivery_attempts: r.delivery_attempts,
                        thread_uuid: None,
                        in_reply_to_uuid: None,
                    },
                    first_for_recipient: r.first_for_recipient,
                })
                .collect())
        })
        .await
    }

    pub async fn insert_failed_send(
        &self,
        attempted_by_session_uuid: Option<uuid::Uuid>,
        attempted_to: &str,
        reason: &'static str,
        request_id: Option<i64>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("mailbox_insert_failed_send", async move {
            let row = sqlx::query!(
                r#"INSERT INTO mailbox.failed_sends
                       (attempted_by_session_id, attempted_to, reason, request_id)
                   VALUES (
                       (SELECT id FROM sessions.sessions WHERE session_uuid = $1),
                       $2, $3, $4
                   )
                   RETURNING id"#,
                attempted_by_session_uuid,
                attempted_to,
                reason,
                request_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(row.id)
        })
        .await
    }

    pub async fn dead_letter_expired_undelivered(
        &self,
        cutoff: DateTime<Utc>,
        activity_cutoff: DateTime<Utc>,
    ) -> Result<ReapOutcome, PersistError> {
        crate::metrics::time_db("mailbox_dead_letter_expired_undelivered", async move {
            let mut tx = self.pool.begin().await?;
            let rows: Vec<uuid::Uuid> = sqlx::query_scalar(
                r#"WITH reaped AS (
                       UPDATE mailbox.messages m
                          SET dead_lettered_at = now()
                        WHERE m.to_session_id IS NOT NULL
                          AND m.sent_request_id IS NULL
                          AND m.channel_delivered_at IS NULL
                          AND m.consumed_at IS NULL
                          AND m.dead_lettered_at IS NULL
                          AND m.queued_at < $1
                          AND NOT EXISTS (
                              SELECT 1 FROM requests.requests r
                               WHERE r.session_id = m.to_session_id
                                 AND r.created_at >= $2
                          )
                    RETURNING m.message_uuid, m.from_session_id, m.to_session_id
                   ),
                   failed AS (
                       INSERT INTO mailbox.failed_sends
                           (attempted_by_session_id, attempted_to, reason, request_id)
                       SELECT r.from_session_id, s.session_uuid::text, 'strand_ttl_expired', NULL
                         FROM reaped r
                         JOIN sessions.sessions s ON s.id = r.to_session_id
                       RETURNING id
                   )
                   SELECT message_uuid FROM reaped"#,
            )
            .bind(cutoff)
            .bind(activity_cutoff)
            .fetch_all(&mut *tx)
            .await?;
            super::undelivered::tell_listed_senders(&mut *tx, &rows).await?;
            tx.commit().await?;
            Ok(ReapOutcome {
                dead_lettered: rows.len() as i64,
            })
        })
        .await
    }

    pub async fn is_first_mail_for_recipient(
        &self,
        message_uuid: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_is_first_for_recipient", async move {
            let first = sqlx::query_scalar!(
                r#"SELECT NOT EXISTS (
                       SELECT 1
                         FROM mailbox.messages m
                         JOIN mailbox.messages p
                           ON p.to_session_id = m.to_session_id AND p.id < m.id
                        WHERE m.message_uuid = $1
                   ) AS "first_for_recipient!""#,
                message_uuid,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(first)
        })
        .await
    }
}
