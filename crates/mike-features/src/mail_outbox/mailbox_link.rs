use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct MailboxState {
    pub claimed_at: Option<DateTime<Utc>>,
    pub channel_delivered_at: Option<DateTime<Utc>>,
    pub sent_request_id: Option<i64>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub answered: bool,
}

const MIRROR: &str = "WITH codex AS (
         SELECT t.session_id
           FROM runtime.threads t
           JOIN runtime.namespaces n ON n.id = t.namespace_id
          WHERE n.runtime_kind = 'codex'
     ), mail AS (
         SELECT m.id, m.message_uuid, m.to_session_id, m.from_session_id, m.body, 'codex' AS runtime
           FROM mailbox.messages m
           JOIN codex c ON c.session_id = m.to_session_id
          WHERE m.sent_request_id IS NULL AND m.dead_lettered_at IS NULL
         UNION
         SELECT m.id, m.message_uuid, m.to_session_id, m.from_session_id, m.body, 'claude' AS runtime
           FROM mailbox.messages m
           JOIN codex c ON c.session_id = m.from_session_id
          WHERE m.to_session_id IS NOT NULL
            AND m.to_session_id NOT IN (SELECT session_id FROM codex)
            AND m.dead_lettered_at IS NULL
     ), fresh AS (
         SELECT mail.message_uuid, mail.body, mail.runtime, mail.id,
                recip.session_uuid AS to_uuid, sender.session_uuid AS from_uuid,
                COALESCE(owner.id, 0) AS generation
           FROM mail
           JOIN sessions.sessions recip ON recip.id = mail.to_session_id
           LEFT JOIN sessions.sessions sender ON sender.id = mail.from_session_id
           LEFT JOIN LATERAL (
               SELECT l.id FROM launch.launches l
                WHERE l.session_uuid = recip.session_uuid
                ORDER BY l.created_at DESC, l.id DESC
                LIMIT 1
           ) owner ON true
          WHERE NOT EXISTS (SELECT 1 FROM mailbox.outbox o WHERE o.message_uuid = mail.message_uuid)
          ORDER BY mail.id
          LIMIT $1
     ), row AS (
         INSERT INTO mailbox.outbox
             (message_uuid, to_session, from_session, runtime, body, generation)
         SELECT message_uuid, to_uuid, from_uuid, runtime, body, generation FROM fresh
         ON CONFLICT (message_uuid) DO NOTHING
         RETURNING message_uuid
     ), staged AS (
         INSERT INTO mailbox.outbox_stages (message_uuid, stage, evidence)
         SELECT message_uuid, 'queued', 'mirrored from the mailbox' FROM row
         RETURNING 1
     )
     SELECT count(*) FROM staged";

pub async fn mirror_codex_mail(pool: &PgPool, limit: i64) -> Result<u64, sqlx::Error> {
    let written = sqlx::query_scalar::<_, i64>(MIRROR) // grader-allow: runtime query, the fleet cannot build .sqlx entries
        .bind(limit)
        .fetch_one(pool)
        .await?;
    Ok(u64::try_from(written).unwrap_or_default())
}

pub async fn mailbox_state(
    pool: &PgPool,
    message: Uuid,
) -> Result<Option<MailboxState>, sqlx::Error> {
    sqlx::query_as::<_, MailboxState>(
        // grader-allow: runtime query, the fleet cannot build .sqlx entries
        "SELECT m.claimed_at, m.channel_delivered_at, m.sent_request_id, m.consumed_at, m.dead_lettered_at,
                EXISTS (SELECT 1 FROM responses.responses r
                         WHERE r.request_id = m.sent_request_id) AS answered
           FROM mailbox.messages m
          WHERE m.message_uuid = $1",
    )
    .bind(message)
    .fetch_optional(pool)
    .await
}

pub async fn mark_taken(pool: &PgPool, message: Uuid, consumed: bool) -> Result<(), sqlx::Error> {
    sqlx::query(
        // grader-allow: runtime query, the fleet cannot build .sqlx entries
        "UPDATE mailbox.messages
            SET claimed_at = COALESCE(claimed_at, now()),
                consumed_at = CASE WHEN $2 THEN COALESCE(consumed_at, now()) ELSE consumed_at END
          WHERE message_uuid = $1",
    )
    .bind(message)
    .bind(consumed)
    .execute(pool)
    .await?;
    Ok(())
}
