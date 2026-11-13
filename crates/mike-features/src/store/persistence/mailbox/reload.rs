use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

use super::types::{InboxCounts, MailboxMessageRow, ReloadedMailboxRow};

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn fetch_inbox_counts(
        self,
        session_id: i64,
    ) -> Result<InboxCounts, PersistError> {
        crate::metrics::time_db("mailbox_fetch_inbox_counts", async move {
            let row = sqlx::query!(
                r#"SELECT
                       COUNT(*) FILTER (
                           WHERE to_session_id = $1
                             AND consumed_at IS NULL
                             AND dead_lettered_at IS NULL
                             AND sent_at IS NOT NULL
                       ) AS "unconsumed!",
                       COUNT(*) FILTER (
                           WHERE to_session_id = $1 AND claimed_at IS NULL
                       ) AS "unclaimed!",
                       COUNT(*) FILTER (
                           WHERE from_session_id = $1 AND dead_lettered_at IS NOT NULL
                       ) AS "dead_lettered_outbox!"
                   FROM mailbox.messages
                  WHERE to_session_id = $1 OR from_session_id = $1"#,
                session_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(InboxCounts {
                unconsumed: row.unconsumed,
                unclaimed: row.unclaimed,
                dead_lettered_outbox: row.dead_lettered_outbox,
            })
        })
        .await
    }

    pub async fn fetch_pending_unclaimed(
        self,
        to_session_uuid: uuid::Uuid,
        limit: i64,
    ) -> Result<Vec<ReloadedMailboxRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_pending_unclaimed", async move {
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
                    AND m.sent_request_id IS NULL
                    AND m.channel_delivered_at IS NULL
                    AND m.claimed_at IS NULL
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

    pub async fn fetch_stale_claimed(
        self,
        to_session_uuid: uuid::Uuid,
        older_than: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<ReloadedMailboxRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_stale_claimed", async move {
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
                    AND m.sent_request_id IS NULL
                    AND m.channel_delivered_at IS NULL
                    AND m.claimed_at IS NOT NULL
                    AND m.claimed_at < $2
                    AND m.consumed_at IS NULL
                    AND m.dead_lettered_at IS NULL
                  ORDER BY m.id ASC
                  LIMIT $3"#,
                to_session_uuid,
                older_than,
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

    pub async fn fetch_all_pending_unclaimed(
        self,
        limit: i64,
    ) -> Result<Vec<ReloadedMailboxRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_all_pending_unclaimed", async move {
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
                  WHERE m.sent_request_id IS NULL
                    AND m.channel_delivered_at IS NULL
                    AND m.claimed_at IS NULL
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

    pub async fn fetch_all_stale_claimed(
        self,
        older_than: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<ReloadedMailboxRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_all_stale_claimed", async move {
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
                  WHERE m.sent_request_id IS NULL
                    AND m.channel_delivered_at IS NULL
                    AND m.claimed_at IS NOT NULL
                    AND m.claimed_at < $1
                    AND m.consumed_at IS NULL
                    AND m.dead_lettered_at IS NULL
                  ORDER BY m.id ASC
                  LIMIT $2"#,
                older_than,
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

    pub async fn has_pending_inbound_mail(
        self,
        to_session_uuid: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_has_pending_inbound", async move {
            let exists = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                       SELECT 1
                         FROM mailbox.messages m
                         JOIN sessions.sessions recip ON recip.id = m.to_session_id
                        WHERE recip.session_uuid = $1
                          AND m.sent_request_id IS NULL
                          AND m.channel_delivered_at IS NULL
                          AND m.claimed_at IS NULL
                          AND m.consumed_at IS NULL
                          AND m.dead_lettered_at IS NULL
                   ) AS "exists!""#,
                to_session_uuid,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(exists)
        })
        .await
    }
}
