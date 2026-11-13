use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{HistoryEntry, WorkThreadError};

pub type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LockedThread {
    pub id: i64,
}

pub struct NewLogEntry<'a> {
    pub thread_id: i64,
    pub kind: &'a str,
    pub session_uuid: Option<Uuid>,
    pub commit_sha: Option<&'a str>,
    pub summary: &'a str,
    pub body: &'a str,
    pub data: Option<&'a Value>,
}

pub async fn lock_thread(
    tx: &mut Tx<'_>,
    project_id: i64,
    slug: &str,
) -> Result<Option<LockedThread>, WorkThreadError> {
    Ok(sqlx::query_as!(
        LockedThread,
        "SELECT id FROM work.threads WHERE project_id = $1 AND slug = $2 FOR UPDATE",
        project_id,
        slug,
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn require_thread(
    tx: &mut Tx<'_>,
    project_id: i64,
    slug: &str,
) -> Result<LockedThread, WorkThreadError> {
    lock_thread(tx, project_id, slug).await?.ok_or_else(|| {
        WorkThreadError::NotFound(format!(
            "thread `{slug}` does not exist in this project; work_thread_list shows the threads that do"
        ))
    })
}

pub async fn insert_thread(
    tx: &mut Tx<'_>,
    project_id: i64,
    slug: &str,
    title: &str,
) -> Result<Option<LockedThread>, WorkThreadError> {
    Ok(sqlx::query_as!(
        LockedThread,
        "INSERT INTO work.threads (project_id, slug, title, status)
         VALUES ($1, $2, $3, 'active')
         ON CONFLICT (project_id, slug) DO NOTHING
         RETURNING id",
        project_id,
        slug,
        title,
    )
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn insert_log(
    tx: &mut Tx<'_>,
    entry: NewLogEntry<'_>,
) -> Result<HistoryEntry, WorkThreadError> {
    Ok(sqlx::query_as!(
        HistoryEntry,
        "INSERT INTO work.thread_log
             (thread_id, ord, session_uuid, commit_sha, summary, kind, body, data)
         SELECT $1::bigint, COALESCE(MAX(l.ord), 0) + 1, $2::uuid, $3::text, $4::text, $5::text,
                $6::text, $7::jsonb
           FROM work.thread_log l WHERE l.thread_id = $1
         RETURNING ord, kind, entry_date, session_uuid, commit_sha, summary, body, data, created_at",
        entry.thread_id,
        entry.session_uuid,
        entry.commit_sha,
        entry.summary,
        entry.kind,
        entry.body,
        entry.data,
    )
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn touch_thread(
    tx: &mut Tx<'_>,
    thread_id: i64,
    session: Option<Uuid>,
) -> Result<(), WorkThreadError> {
    sqlx::query!(
        "UPDATE work.threads
            SET updated_at = now(), last_session_uuid = COALESCE($2, last_session_uuid)
          WHERE id = $1",
        thread_id,
        session,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn link_session(
    tx: &mut Tx<'_>,
    thread_id: i64,
    session: Uuid,
    contribution: Option<&str>,
) -> Result<(), WorkThreadError> {
    sqlx::query!(
        "INSERT INTO work.thread_sessions (thread_id, session_uuid, contribution)
         VALUES ($1, $2, COALESCE($3, ''))
         ON CONFLICT (thread_id, session_uuid) DO UPDATE
            SET last_seen_at = now(),
                contribution = COALESCE($3, work.thread_sessions.contribution)",
        thread_id,
        session,
        contribution,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
