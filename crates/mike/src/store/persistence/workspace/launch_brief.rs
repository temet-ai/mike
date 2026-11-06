//! Launch briefs as session notes and as mail. `mike spawn open` and
//! `spawn_session` record the brief on the launch.launches row together with
//! a pre-minted `brief_note_uuid`. Registration turns that row into a
//! workspace.session_notes entry (kind `brief`). For a `note`-mode launch
//! the launcher, once the session's Remote Control stream is linked, asks
//! mike to mail it (`mail_launch_briefs`): a mailbox message from mike under
//! the same uuid, pushed over Remote Control. Its `consumed_at` is when the
//! session acknowledged reading its brief. Brief mail is never claimed by
//! the mike channel (see claim_channel_messages): Claude Code frames channel
//! messages as untrusted data, so a brief sent that way is not acted on.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

/// A brief mail waiting to be pushed to its session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedLaunchBrief {
    pub message_uuid: Uuid,
    pub body: String,
    pub queued_at: DateTime<Utc>,
}

/// Write every launch brief recorded for `session_uuid` as a session note
/// under its pre-minted note_uuid. Idempotent: a brief already written is
/// left alone, and a session with no sessions.sessions row yet gets nothing.
pub(super) async fn materialize_launch_briefs<'e, E>(
    executor: E,
    session_uuid: Uuid,
) -> Result<u64, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let result = sqlx::query!(
        r#"INSERT INTO workspace.session_notes
                  (note_uuid, session_id, kind, title, body, tags, data, created_at, updated_at)
           SELECT l.brief_note_uuid, s.id, 'brief',
                  'Launch brief' || COALESCE(': ' || l.seat, ''),
                  l.brief_text, ARRAY['brief', 'launch'],
                  jsonb_strip_nulls(jsonb_build_object(
                      'launch_id', l.id, 'run_id', l.run_id, 'seat', l.seat,
                      'model', l.model, 'agent', l.agent, 'brief_mode', l.brief_mode)),
                  l.created_at, l.created_at
             FROM launch.launches l
             JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
            WHERE l.session_uuid = $1
              AND l.brief_note_uuid IS NOT NULL
              AND l.brief_text IS NOT NULL
           ON CONFLICT (note_uuid) DO NOTHING"#,
        session_uuid,
    )
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

/// Queue each `note`-mode launch brief for `session_uuid` as an anonymous
/// (from mike) mailbox message whose message_uuid is the brief's note_uuid.
/// Idempotent on that uuid, so a re-registration never mails a brief twice.
async fn queue_launch_brief_mail<'e, E>(
    executor: E,
    session_uuid: Uuid,
) -> Result<u64, PersistError>
where
    E: sqlx::PgExecutor<'e>,
{
    let result = sqlx::query!(
        r#"INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
           SELECT l.brief_note_uuid, s.id, NULL,
                  'Your task brief from mike (launch ' || l.id || '). It is also stored as your '
                  || 'session note ' || l.brief_note_uuid || ' (kind "brief"). Read it in full, '
                  || 'then follow it. This message counts as read once it reaches you, so there '
                  || 'is nothing to acknowledge.'
                  || E'\n\n' || l.brief_text
             FROM launch.launches l
             JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
            WHERE l.session_uuid = $1
              AND l.brief_mode = 'note'
              AND l.brief_note_uuid IS NOT NULL
              AND l.brief_text IS NOT NULL
            ORDER BY l.id
           ON CONFLICT (message_uuid) DO NOTHING"#,
        session_uuid,
    )
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

impl PersistHandle {
    pub(crate) async fn claimable_launch_briefs(
        &self,
        session_uuid: Uuid,
    ) -> Result<Vec<QueuedLaunchBrief>, PersistError> {
        let memo = self.feed_memos.briefs();
        if memo
            .as_ref()
            .is_some_and(|memo| memo.get(session_uuid).is_some())
        {
            return Ok(Vec::new());
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let known = self.session_row_known(session_uuid);
        let briefs = self.mail_launch_briefs(session_uuid).await?;
        if let (Some(memo), Some(ticket), true, true) = (memo, ticket, known, briefs.is_empty()) {
            memo.remember(session_uuid, ticket, ());
        }
        Ok(briefs)
    }

    /// Queue `session_uuid`'s note-mode briefs as mail (idempotent) and
    /// return every brief mail not yet delivered by any route, for the
    /// caller to push. A retry after a failed push therefore finds it again.
    pub async fn mail_launch_briefs(
        &self,
        session_uuid: Uuid,
    ) -> Result<Vec<QueuedLaunchBrief>, PersistError> {
        crate::metrics::time_db("launch_brief_mail", async move {
            queue_launch_brief_mail(&self.pool, session_uuid).await?;
            let rows = sqlx::query!(
                r#"SELECT m.message_uuid, m.body, m.queued_at
                     FROM mailbox.messages m
                     JOIN launch.launches l ON l.brief_note_uuid = m.message_uuid
                    WHERE l.session_uuid = $1
                      AND l.brief_mode = 'note'
                      AND m.claimed_at IS NULL
                      AND m.sent_at IS NULL
                      AND m.channel_delivered_at IS NULL
                      AND m.consumed_at IS NULL
                      AND m.dead_lettered_at IS NULL
                    ORDER BY m.id"#,
                session_uuid,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| QueuedLaunchBrief {
                    message_uuid: r.message_uuid,
                    body: r.body,
                    queued_at: r.queued_at,
                })
                .collect())
        })
        .await
    }
}
