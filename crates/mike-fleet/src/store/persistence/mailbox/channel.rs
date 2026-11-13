use crate::store::persistence::PersistError;

use super::types::ClaimedChannelMessageRow;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn claim_channel_messages(
        self,
        to_session_uuid: uuid::Uuid,
        limit: i64,
    ) -> Result<Vec<ClaimedChannelMessageRow>, PersistError> {
        crate::metrics::time_db("mailbox_claim_channel_messages", async move {
            let rows = sqlx::query!(
                r#"WITH to_claim AS (
                       SELECT m.id
                         FROM mailbox.messages m
                         JOIN sessions.sessions recip ON recip.id = m.to_session_id
                        WHERE recip.session_uuid = $1
                          AND m.claimed_at IS NULL
                          AND m.sent_request_id IS NULL
                          AND m.channel_delivered_at IS NULL
                          AND m.consumed_at IS NULL
                          AND m.dead_lettered_at IS NULL
                          -- Launch-brief mail is pushed over Remote Control only:
                          -- Claude Code frames a channel message as untrusted data
                          -- and will not act on a brief delivered that way.
                          AND NOT EXISTS (
                              SELECT 1 FROM launch.launches l
                               WHERE l.brief_note_uuid = m.message_uuid)
                        ORDER BY m.id ASC
                        LIMIT $2
                        FOR UPDATE OF m
                   ),
                   claimed AS (
                       UPDATE mailbox.messages m
                          SET channel_delivered_at = now(),
                              delivery_attempts = m.delivery_attempts + 1
                         FROM to_claim
                        WHERE m.id = to_claim.id
                    RETURNING m.id, m.message_uuid, m.to_session_id, m.from_session_id,
                              m.body, m.queued_at, m.delivery_attempts, m.thread_id,
                              m.in_reply_to_id
                   )
                   SELECT
                       c.message_uuid       AS "message_uuid!",
                       recip.session_uuid   AS "to_session_uuid!",
                       sender.session_uuid  AS "from_session_uuid?",
                       sg.display_name      AS "from_display_name?",
                       sg.role              AS "from_role?",
                       c.body               AS "body!",
                       c.queued_at          AS "queued_at!",
                       c.delivery_attempts  AS "delivery_attempts!",
                       th.thread_uuid       AS "thread_uuid?",
                       parent.message_uuid  AS "in_reply_to_uuid?",
                       c.id = (
                           SELECT MIN(p.id) FROM mailbox.messages p
                            WHERE p.to_session_id = c.to_session_id
                       ) AS "first_for_recipient!"
                   FROM claimed c
                   JOIN sessions.sessions recip ON recip.id = c.to_session_id
                   LEFT JOIN sessions.sessions sender ON sender.id = c.from_session_id
                   LEFT JOIN sessions.session_goals sg ON sg.session_id = c.from_session_id
                   LEFT JOIN mailbox.threads th ON th.id = c.thread_id
                   LEFT JOIN mailbox.messages parent ON parent.id = c.in_reply_to_id
                  ORDER BY c.id ASC"#,
                to_session_uuid,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            let mut claimed = Vec::with_capacity(rows.len());
            for r in rows {
                let from_label = match r.from_session_uuid {
                    Some(from) => self.sender_label(from).await,
                    None => None,
                };
                claimed.push(ClaimedChannelMessageRow {
                    message_uuid: r.message_uuid,
                    to_session_uuid: r.to_session_uuid,
                    from_session_uuid: r.from_session_uuid,
                    from_display_name: r.from_display_name,
                    from_role: r.from_role,
                    body: r.body,
                    queued_at: r.queued_at,
                    delivery_attempts: r.delivery_attempts,
                    thread_uuid: r.thread_uuid,
                    in_reply_to_uuid: r.in_reply_to_uuid,
                    first_for_recipient: r.first_for_recipient,
                    from_label,
                });
            }
            Ok(claimed)
        })
        .await
    }

    pub async fn release_channel_messages(
        self,
        to_session_uuid: uuid::Uuid,
        message_uuids: Vec<uuid::Uuid>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("mailbox_release_channel_messages", async move {
            let result = sqlx::query!(
                "UPDATE mailbox.messages m
                    SET channel_delivered_at = NULL
                   FROM sessions.sessions recip
                  WHERE recip.id = m.to_session_id
                    AND recip.session_uuid = $1
                    AND m.message_uuid = ANY($2)
                    AND m.channel_delivered_at IS NOT NULL
                    AND m.consumed_at IS NULL
                    AND m.dead_lettered_at IS NULL",
                to_session_uuid,
                &message_uuids,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }
}
