use crate::store::persistence::{PersistError, PersistHandle};

use super::types::{AckOutcome, InsertedMessage, RedeliveryOutcome, SessionIdResolution};

fn uuid_prefix_bounds(prefix: &str) -> Option<(uuid::Uuid, uuid::Uuid)> {
    const LOWEST: &str = "00000000-0000-0000-0000-000000000000";
    const HIGHEST: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    if prefix.len() > LOWEST.len()
        || prefix
            .bytes()
            .any(|b| !b.is_ascii() || b.is_ascii_uppercase())
    {
        return None;
    }
    let first = uuid::Uuid::try_parse(&format!("{prefix}{}", &LOWEST[prefix.len()..])).ok()?;
    let last = uuid::Uuid::try_parse(&format!("{prefix}{}", &HIGHEST[prefix.len()..])).ok()?;
    Some((first, last))
}

impl PersistHandle {
    pub async fn insert_message(
        &self,
        message_uuid: uuid::Uuid,
        to_session_uuid: uuid::Uuid,
        from_session_uuid: Option<uuid::Uuid>,
        body: &str,
    ) -> Result<Option<InsertedMessage>, PersistError> {
        self.insert_message_with_context(
            message_uuid,
            to_session_uuid,
            from_session_uuid,
            body,
            &[],
        )
        .await
    }

    pub(crate) async fn resolve_role_recipients(
        &self,
        project_id: i64,
        role: &str,
        sender_session_uuid: uuid::Uuid,
    ) -> Result<Vec<uuid::Uuid>, PersistError> {
        crate::metrics::time_db("mailbox_resolve_role_recipients", async move {
            self.recipients_labelled(project_id, "role", role, sender_session_uuid)
                .await
        })
        .await
    }

    pub async fn resolve_name_recipient(
        &self,
        project_id: i64,
        name: &str,
        sender_session_uuid: uuid::Uuid,
    ) -> Result<SessionIdResolution, PersistError> {
        crate::metrics::time_db("mailbox_resolve_name_recipient", async move {
            let mut rows = self
                .recipients_labelled(project_id, "display_name", name, sender_session_uuid)
                .await?;
            Ok(match rows.len() {
                0 => SessionIdResolution::NotFound,
                1 => SessionIdResolution::Resolved(rows.swap_remove(0)),
                _ => SessionIdResolution::Ambiguous(rows),
            })
        })
        .await
    }

    async fn recipients_labelled(
        &self,
        project_id: i64,
        label: &str,
        value: &str,
        sender_session_uuid: uuid::Uuid,
    ) -> Result<Vec<uuid::Uuid>, PersistError> {
        Ok(sqlx::query_file_scalar!(
            "src/store/persistence/mailbox/resolve_recipients.sql",
            project_id,
            value,
            sender_session_uuid,
            label,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn resolve_session_id_prefix(
        &self,
        prefix: &str,
    ) -> Result<SessionIdResolution, PersistError> {
        crate::metrics::time_db("mailbox_resolve_session_id_prefix", async move {
            let Some((first, last)) = uuid_prefix_bounds(prefix) else {
                return Ok(SessionIdResolution::NotFound);
            };
            let mut rows = sqlx::query_scalar!(
                "SELECT session_uuid FROM sessions.sessions WHERE session_uuid BETWEEN $1 AND $2",
                first,
                last,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(match rows.len() {
                0 => SessionIdResolution::NotFound,
                1 => SessionIdResolution::Resolved(rows.swap_remove(0)),
                _ => SessionIdResolution::Ambiguous(rows),
            })
        })
        .await
    }

    pub async fn claim_message(&self, message_uuid: uuid::Uuid) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_claim_message", async move {
            let rows = sqlx::query!(
                "UPDATE mailbox.messages
                    SET claimed_at = now()
                  WHERE message_uuid = $1
                    AND claimed_at IS NULL
                    AND channel_delivered_at IS NULL
                    AND consumed_at IS NULL
                    AND dead_lettered_at IS NULL",
                message_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(rows.rows_affected() > 0)
        })
        .await
    }

    pub async fn unclaim_message(&self, message_uuid: uuid::Uuid) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_unclaim_message", async move {
            let rows = sqlx::query!(
                "UPDATE mailbox.messages
                    SET claimed_at = NULL
                  WHERE message_uuid = $1
                    AND sent_request_id IS NULL
                    AND consumed_at IS NULL
                    AND dead_lettered_at IS NULL",
                message_uuid,
            )
            .execute(&self.pool)
            .await?;
            let released = rows.rows_affected() > 0;
            if released {
                self.feed_memos.forget_briefs();
            }
            Ok(released)
        })
        .await
    }

    pub async fn backfill_mailbox_sent_request_id(
        &self,
        message_uuid: uuid::Uuid,
        request_id: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_backfill_sent_request_id", async move {
            let rows = sqlx::query!(
                "UPDATE mailbox.messages
                    SET sent_request_id   = $1,
                        sent_at           = now(),
                        delivery_attempts = delivery_attempts + 1
                  WHERE message_uuid = $2
                    AND sent_request_id IS NULL
                    AND channel_delivered_at IS NULL",
                request_id,
                message_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(rows.rows_affected() > 0)
        })
        .await
    }

    pub async fn stamp_mailbox_sent_without_request(
        &self,
        message_uuid: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_stamp_sent_without_request", async move {
            let rows = sqlx::query!(
                "UPDATE mailbox.messages
                    SET sent_at           = now(),
                        delivery_attempts = delivery_attempts + 1
                  WHERE message_uuid = $1
                    AND sent_request_id IS NULL
                    AND sent_at IS NULL
                    AND channel_delivered_at IS NULL",
                message_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(rows.rows_affected() > 0)
        })
        .await
    }

    pub async fn consume_client_claimed(
        &self,
        message_uuid: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_consume_client_claimed", async move {
            let rows = sqlx::query!(
                "UPDATE mailbox.messages
                    SET sent_at           = COALESCE(sent_at, now()),
                        consumed_at       = now(),
                        delivery_attempts = delivery_attempts + 1
                  WHERE message_uuid = $1
                    AND claimed_at IS NOT NULL
                    AND consumed_at IS NULL
                    AND dead_lettered_at IS NULL",
                message_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(rows.rows_affected() > 0)
        })
        .await
    }

    pub async fn bump_redelivery_attempt(
        &self,
        message_uuid: uuid::Uuid,
        max_attempts: i32,
        backoff_base_secs: i64,
        backoff_cap_secs: i64,
    ) -> Result<Option<RedeliveryOutcome>, PersistError> {
        crate::metrics::time_db("mailbox_bump_redelivery_attempt", async move {
            let row = sqlx::query!(
                r#"WITH snap AS (
                       SELECT delivery_attempts, next_retry_at
                         FROM mailbox.messages
                        WHERE message_uuid = $1
                          AND sent_at IS NOT NULL
                          AND consumed_at IS NULL
                          AND dead_lettered_at IS NULL
                        FOR UPDATE
                   ),
                   decided AS (
                       SELECT delivery_attempts,
                              (next_retry_at IS NULL OR next_retry_at <= now()) AS eligible
                         FROM snap
                   )
                   UPDATE mailbox.messages m
                      SET delivery_attempts = CASE
                              WHEN d.eligible THEN d.delivery_attempts + 1
                              ELSE d.delivery_attempts
                          END,
                          next_retry_at = CASE
                              WHEN d.eligible THEN now() + (
                                  LEAST(
                                      $3::double precision * power(2::double precision, (d.delivery_attempts - 1)::double precision),
                                      $4::double precision
                                  ) * interval '1 second'
                              )
                              ELSE m.next_retry_at
                          END,
                          dead_lettered_at = CASE
                              WHEN d.eligible AND d.delivery_attempts + 1 >= $2 THEN now()
                              ELSE m.dead_lettered_at
                          END
                     FROM decided d
                    WHERE m.message_uuid = $1
                RETURNING m.delivery_attempts AS "delivery_attempts!",
                          (m.dead_lettered_at IS NOT NULL) AS "dead_lettered!",
                          d.eligible AS "attempted!""#,
                message_uuid,
                max_attempts,
                backoff_base_secs as f64,
                backoff_cap_secs as f64,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| RedeliveryOutcome {
                delivery_attempts: r.delivery_attempts,
                dead_lettered: r.dead_lettered,
                attempted: r.attempted,
            }))
        })
        .await
    }

    pub async fn ack_message(
        &self,
        to_session_uuid: uuid::Uuid,
        message_uuid: uuid::Uuid,
    ) -> Result<AckOutcome, PersistError> {
        crate::metrics::time_db("mailbox_ack_message", async move {
            let updated = sqlx::query_scalar!(
                r#"UPDATE mailbox.messages m
                      SET consumed_at = now()
                     FROM sessions.sessions recip
                    WHERE m.message_uuid = $2
                      AND recip.id = m.to_session_id
                      AND recip.session_uuid = $1
                      AND m.consumed_at IS NULL
                      AND (m.sent_request_id IS NOT NULL OR m.channel_delivered_at IS NOT NULL)
                RETURNING m.consumed_at AS "consumed_at!""#,
                to_session_uuid,
                message_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            if let Some(consumed_at) = updated {
                return Ok(AckOutcome::Consumed(consumed_at));
            }
            let existing = sqlx::query_scalar!(
                r#"SELECT m.consumed_at AS "consumed_at?"
                     FROM mailbox.messages m
                     JOIN sessions.sessions recip ON recip.id = m.to_session_id
                    WHERE m.message_uuid = $2 AND recip.session_uuid = $1"#,
                to_session_uuid,
                message_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(match existing {
                None => AckOutcome::NotFound,
                Some(Some(consumed_at)) => AckOutcome::Consumed(consumed_at),
                Some(None) => AckOutcome::NeverDelivered,
            })
        })
        .await
    }
}

#[cfg(test)]
mod prefix_tests {
    use super::uuid_prefix_bounds;

    #[test]
    fn uuid_prefix_bounds_short_hex_spans_the_prefix_range() -> anyhow::Result<()> {
        let (first, last) =
            uuid_prefix_bounds("b4ce").ok_or_else(|| anyhow::anyhow!("hex prefix has bounds"))?;
        assert_eq!(first.to_string(), "b4ce0000-0000-0000-0000-000000000000");
        assert_eq!(last.to_string(), "b4ceffff-ffff-ffff-ffff-ffffffffffff");
        Ok(())
    }

    #[test]
    fn uuid_prefix_bounds_full_uuid_collapses_to_one_value() -> anyhow::Result<()> {
        let id = "b4ce1a3c-0000-4000-8000-000000000001";
        let (first, last) =
            uuid_prefix_bounds(id).ok_or_else(|| anyhow::anyhow!("full uuid has bounds"))?;
        assert_eq!(first, last);
        assert_eq!(first.to_string(), id);
        Ok(())
    }

    #[test]
    fn uuid_prefix_bounds_rejects_text_no_canonical_uuid_starts_with() {
        assert!(uuid_prefix_bounds("B4CE").is_none());
        assert!(uuid_prefix_bounds("xyz").is_none());
        assert!(uuid_prefix_bounds("b4ce-").is_none());
        assert!(uuid_prefix_bounds(&"a".repeat(37)).is_none());
    }
}
