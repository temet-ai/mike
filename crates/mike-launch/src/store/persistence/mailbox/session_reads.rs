use crate::store::persistence::PersistError;

use super::types::{
    ContextItem, MailboxForSessionWithActivity, MailboxMessageRow, MailboxMessageWithActivity,
    ReceiptEvidenceRow, StoredMessageRow,
};

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn fetch_for_session_with_activity(
        self,
        session_uuid: uuid::Uuid,
    ) -> Result<MailboxForSessionWithActivity, PersistError> {
        crate::metrics::time_db("mailbox_fetch_for_session_with_activity", async move {
            let Some(session_id) = self.session_row_id(session_uuid).await? else {
                return Ok(MailboxForSessionWithActivity {
                    inbox: Vec::new(),
                    outbox: Vec::new(),
                });
            };
            let mail = self.session_mail(session_uuid, session_id).await?;
            let inbox = mail.inbox.clone();
            let outbox = mail.outbox.clone();
            let mut recipients: Vec<(uuid::Uuid, i64)> = inbox
                .iter()
                .chain(&outbox)
                .filter(|message| message.awaiting_delivery())
                .map(|message| (message.to_session_uuid, message.to_session_id))
                .collect();
            recipients.sort_unstable();
            recipients.dedup();
            let activity = self.recipient_activity(&recipients).await?;
            let attach = |messages: Vec<StoredMessageRow>| -> Vec<MailboxMessageWithActivity> {
                messages
                    .into_iter()
                    .map(|message| {
                        let seen = message
                            .awaiting_delivery()
                            .then(|| activity.get(&message.to_session_id).copied())
                            .flatten()
                            .unwrap_or_default();
                        message.with_activity(seen.last_request_at, seen.last_eligible_turn_at)
                    })
                    .collect()
            };
            Ok(MailboxForSessionWithActivity {
                inbox: attach(inbox),
                outbox: attach(outbox),
            })
        })
        .await
    }

    pub(super) async fn fetch_inbox_messages(
        self,
        recipient_id: i64,
    ) -> Result<Vec<StoredMessageRow>, PersistError> {
        Ok(sqlx::query_as!(
            StoredMessageRow,
            r#"SELECT
                   m.message_uuid,
                   recip.session_uuid  AS to_session_uuid,
                   sender.session_uuid AS "from_session_uuid?",
                   m.body,
                   m.queued_at,
                   m.claimed_at,
                   m.sent_request_id,
                   m.sent_at,
                   m.consumed_at,
                   m.dead_lettered_at,
                   m.channel_delivered_at,
                   m.delivery_attempts,
                   m.to_session_id AS "to_session_id!",
                   COALESCE(ca.items, '[]'::jsonb)
                       AS "context_items!: sqlx::types::Json<Vec<ContextItem>>",
                   th.thread_uuid AS "thread_uuid?",
                   parent.message_uuid AS "in_reply_to_uuid?"
               FROM mailbox.messages m
               JOIN sessions.sessions recip ON recip.id = m.to_session_id
               LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
               LEFT JOIN LATERAL (
                   SELECT jsonb_agg(
                              jsonb_build_object('kind', ci.kind, 'value', ci.value, 'label', ci.label)
                              ORDER BY ci.id
                          ) AS items
                     FROM mailbox.message_context_items ci
                    WHERE ci.message_id = m.id
               ) ca ON true
               LEFT JOIN mailbox.threads th ON th.id = m.thread_id
               LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
              WHERE m.to_session_id = $1
              ORDER BY m.id ASC"#,
            recipient_id,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(super) async fn fetch_outbox_messages(
        self,
        sender_id: i64,
    ) -> Result<Vec<StoredMessageRow>, PersistError> {
        Ok(sqlx::query_as!(
            StoredMessageRow,
            r#"SELECT
                   m.message_uuid,
                   recip.session_uuid  AS to_session_uuid,
                   sender.session_uuid AS "from_session_uuid?",
                   m.body,
                   m.queued_at,
                   m.claimed_at,
                   m.sent_request_id,
                   m.sent_at,
                   m.consumed_at,
                   m.dead_lettered_at,
                   m.channel_delivered_at,
                   m.delivery_attempts,
                   m.to_session_id AS "to_session_id!",
                   COALESCE(ca.items, '[]'::jsonb)
                       AS "context_items!: sqlx::types::Json<Vec<ContextItem>>",
                   th.thread_uuid AS "thread_uuid?",
                   parent.message_uuid AS "in_reply_to_uuid?"
               FROM mailbox.messages m
               JOIN sessions.sessions sender ON sender.id = m.from_session_id
               JOIN sessions.sessions recip ON recip.id = m.to_session_id
               LEFT JOIN LATERAL (
                   SELECT jsonb_agg(
                              jsonb_build_object('kind', ci.kind, 'value', ci.value, 'label', ci.label)
                              ORDER BY ci.id
                          ) AS items
                     FROM mailbox.message_context_items ci
                    WHERE ci.message_id = m.id
               ) ca ON true
               LEFT JOIN mailbox.threads th ON th.id = m.thread_id
               LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
              WHERE m.from_session_id = $1
              ORDER BY m.id ASC"#,
            sender_id,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn fetch_message_for_recipient(
        self,
        message_uuid: uuid::Uuid,
        to_session_uuid: uuid::Uuid,
    ) -> Result<Option<MailboxMessageRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_message_for_recipient", async move {
            let row = sqlx::query_as!(
                MailboxMessageRow,
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
                       m.channel_delivered_at AS "channel_delivered_at?",
                       m.delivery_attempts AS "delivery_attempts!",
                       th.thread_uuid       AS "thread_uuid?",
                       parent.message_uuid  AS "in_reply_to_uuid?"
                   FROM mailbox.messages m
                   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
                   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
                   LEFT JOIN mailbox.threads th ON th.id = m.thread_id
                   LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
                  WHERE m.message_uuid = $1 AND recip.session_uuid = $2"#,
                message_uuid,
                to_session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn fetch_receipt_evidence(
        self,
        to_session_uuid: uuid::Uuid,
        needle: &str,
        after: chrono::DateTime<chrono::Utc>,
        limit: i64,
    ) -> Result<Vec<ReceiptEvidenceRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_receipt_evidence", async move {
            let Some(session_id) = self.session_row_id(to_session_uuid).await? else {
                return Ok(Vec::new());
            };
            let rows = sqlx::query_as!(
                ReceiptEvidenceRow,
                r#"SELECT request_id AS "request_id!",
                          created_at AS "created_at!",
                          convert_from(substring(request_body
                                                 from greatest(1, pos - 120) for 400),
                                       'UTF8') AS excerpt
                     FROM (
                         SELECT r.id AS request_id,
                                r.created_at,
                                r.request_body,
                                position(convert_to($2, 'UTF8') in r.request_body) AS pos
                           FROM requests.requests r
                          WHERE r.session_id = $1
                            AND r.created_at >= $3
                            AND r.request_body IS NOT NULL
                            AND position(convert_to($2, 'UTF8') in r.request_body) > 0
                          LIMIT $4
                     ) hits
                    ORDER BY created_at"#,
                session_id,
                needle,
                after,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_message_for_sender(
        self,
        message_uuid: uuid::Uuid,
        from_session_uuid: uuid::Uuid,
    ) -> Result<Option<MailboxMessageRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_message_for_sender", async move {
            let row = sqlx::query_as!(
                MailboxMessageRow,
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
                       m.channel_delivered_at AS "channel_delivered_at?",
                       m.delivery_attempts AS "delivery_attempts!",
                       th.thread_uuid       AS "thread_uuid?",
                       parent.message_uuid  AS "in_reply_to_uuid?"
                   FROM mailbox.messages m
                   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
                   JOIN sessions.sessions sender ON sender.id = m.from_session_id
                   LEFT JOIN mailbox.threads th ON th.id = m.thread_id
                   LEFT JOIN mailbox.messages parent ON parent.id = m.in_reply_to_id
                  WHERE m.message_uuid = $1 AND sender.session_uuid = $2"#,
                message_uuid,
                from_session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}
