use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::WorkThreadError;
use super::lock::{Tx, lock_thread};
use super::validate::invalid;
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Default)]
pub struct ImportedState {
    pub tldr: String,
    pub where_we_are: String,
    pub next_up: String,
    pub blockers: String,
    pub open_questions: String,
    pub decisions: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ImportedEntry {
    pub date: NaiveDate,
    pub session_uuid: Option<Uuid>,
    pub commit_sha: Option<String>,
    pub summary: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ImportedThread {
    pub slug: String,
    pub title: Option<String>,
    pub goal: Option<String>,
    pub status: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub last_session_uuid: Option<Uuid>,
    pub source_path: String,
    pub parse_error: Option<String>,
    pub state: Option<ImportedState>,
    pub acceptance_criteria: Option<String>,
    pub entries: Vec<ImportedEntry>,
    pub original_text: String,
    pub original_date: NaiveDate,
    pub session_uuids: Vec<Uuid>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ImportOutcome {
    Created,
    Updated,
    LeftAlone(String),
}

struct LogRows {
    ords: Vec<i32>,
    dates: Vec<NaiveDate>,
    sessions: Vec<Option<Uuid>>,
    shas: Vec<Option<String>>,
    summaries: Vec<String>,
    kinds: Vec<String>,
    bodies: Vec<String>,
    data: Vec<Value>,
}

fn log_rows(thread: &ImportedThread) -> LogRows {
    let marker = json!({"imported_from": thread.source_path});
    let note_data = json!({"imported_from": thread.source_path, "notes": thread.notes});
    let mut rows = LogRows {
        ords: vec![1],
        dates: vec![thread.original_date],
        sessions: vec![None],
        shas: vec![None],
        summaries: vec![format!("Imported worklog {}", thread.source_path)],
        kinds: vec!["note".to_owned()],
        bodies: vec![thread.original_text.clone()],
        data: vec![note_data],
    };
    for (index, entry) in thread.entries.iter().enumerate() {
        rows.ords.push(i32::try_from(index + 2).unwrap_or(i32::MAX));
        rows.dates.push(entry.date);
        rows.sessions.push(entry.session_uuid);
        rows.shas.push(entry.commit_sha.clone());
        rows.summaries.push(entry.summary.clone());
        rows.kinds.push("checkpoint".to_owned());
        rows.bodies.push(entry.body.clone());
        rows.data.push(marker.clone());
    }
    rows
}

async fn touched_in_mike(tx: &mut Tx<'_>, thread_id: i64) -> Result<bool, WorkThreadError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT t.source_path IS NULL OR EXISTS (
                SELECT 1 FROM work.thread_log l
                 WHERE l.thread_id = t.id AND NOT jsonb_exists(COALESCE(l.data, '{}'::jsonb), 'imported_from')) AS "touched!"
           FROM work.threads t WHERE t.id = $1"#,
        thread_id,
    )
    .fetch_one(&mut **tx)
    .await?)
}

async fn upsert_identity(
    tx: &mut Tx<'_>,
    project_id: i64,
    thread: &ImportedThread,
) -> Result<(i64, bool), WorkThreadError> {
    let row = sqlx::query!(
        r#"INSERT INTO work.threads
             (project_id, slug, title, goal, status, branch, worktree, last_session_uuid,
              source_path, parse_error)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (project_id, slug) DO UPDATE SET
            title = EXCLUDED.title, goal = EXCLUDED.goal, status = EXCLUDED.status,
            branch = EXCLUDED.branch, worktree = EXCLUDED.worktree,
            last_session_uuid = EXCLUDED.last_session_uuid,
            source_path = EXCLUDED.source_path, parse_error = EXCLUDED.parse_error,
            parsed_at = now(), updated_at = now()
         RETURNING id, (old.id IS NULL) AS "inserted!""#,
        project_id,
        &thread.slug,
        thread.title.as_deref(),
        thread.goal.as_deref(),
        thread.status.as_deref(),
        thread.branch.as_deref(),
        thread.worktree.as_deref(),
        thread.last_session_uuid,
        &thread.source_path,
        thread.parse_error.as_deref(),
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok((row.id, row.inserted))
}

async fn upsert_state(
    tx: &mut Tx<'_>,
    thread_id: i64,
    state: &ImportedState,
) -> Result<(), WorkThreadError> {
    sqlx::query!(
        "INSERT INTO work.thread_state
             (thread_id, tldr, where_we_are, next_up, blockers, open_questions, decisions, files)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (thread_id) DO UPDATE SET
            tldr = EXCLUDED.tldr, where_we_are = EXCLUDED.where_we_are, next_up = EXCLUDED.next_up,
            blockers = EXCLUDED.blockers, open_questions = EXCLUDED.open_questions,
            decisions = EXCLUDED.decisions, files = EXCLUDED.files, updated_at = now(),
            source_updated_at = NULL,
            embedding = NULL, embedding_model = NULL, embedded_at = NULL",
        thread_id,
        &state.tldr,
        &state.where_we_are,
        &state.next_up,
        &state.blockers,
        &state.open_questions,
        &state.decisions,
        &state.files,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_criteria(
    tx: &mut Tx<'_>,
    thread_id: i64,
    source_path: &str,
    text: &str,
) -> Result<(), WorkThreadError> {
    sqlx::query!(
        "INSERT INTO work.thread_acceptance_criteria (thread_id, criteria_text, source_path)
         VALUES ($1, $2, $3)
         ON CONFLICT (thread_id) DO UPDATE SET
            criteria_text = EXCLUDED.criteria_text, source_path = EXCLUDED.source_path,
            parsed_at = now(), parse_error = NULL,
            embedding = NULL, embedding_model = NULL, embedded_at = NULL",
        thread_id,
        text,
        source_path,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_log(
    tx: &mut Tx<'_>,
    thread_id: i64,
    thread: &ImportedThread,
) -> Result<(), WorkThreadError> {
    let rows = log_rows(thread);
    sqlx::query!(
        "INSERT INTO work.thread_log
             (thread_id, ord, entry_date, session_uuid, commit_sha, summary, kind, body, data)
         SELECT $1, u.ord, u.entry_date, u.session_uuid, u.commit_sha, u.summary, u.kind, u.body, u.data
           FROM unnest($2::int[], $3::date[], $4::uuid[], $5::text[], $6::text[], $7::text[], $8::text[], $9::jsonb[])
                AS u(ord, entry_date, session_uuid, commit_sha, summary, kind, body, data)
         ON CONFLICT (thread_id, ord) DO UPDATE SET
            entry_date = EXCLUDED.entry_date, session_uuid = EXCLUDED.session_uuid,
            commit_sha = EXCLUDED.commit_sha, summary = EXCLUDED.summary, kind = EXCLUDED.kind,
            body = EXCLUDED.body, data = EXCLUDED.data",
        thread_id,
        &rows.ords,
        &rows.dates,
        &rows.sessions as &[Option<Uuid>],
        &rows.shas as &[Option<String>],
        &rows.summaries,
        &rows.kinds,
        &rows.bodies,
        &rows.data,
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "DELETE FROM work.thread_log
          WHERE thread_id = $1 AND ord > $2 AND jsonb_exists(COALESCE(data, '{}'::jsonb), 'imported_from')",
        thread_id,
        i32::try_from(thread.entries.len() + 1).unwrap_or(i32::MAX),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn link_sessions(
    tx: &mut Tx<'_>,
    thread_id: i64,
    thread: &ImportedThread,
) -> Result<(), WorkThreadError> {
    let mut sessions: Vec<Uuid> = thread.session_uuids.clone();
    sessions.extend(thread.entries.iter().filter_map(|entry| entry.session_uuid));
    sessions.extend(thread.last_session_uuid);
    sessions.sort();
    sessions.dedup();
    sqlx::query!(
        "INSERT INTO work.thread_sessions (thread_id, session_uuid)
         SELECT $1::bigint, s FROM unnest($2::uuid[]) AS s
         ON CONFLICT (thread_id, session_uuid) DO NOTHING",
        thread_id,
        &sessions,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

impl PersistHandle {
    pub async fn work_thread_exists(
        &self,
        project_id: i64,
        thread_slug: &str,
    ) -> Result<bool, WorkThreadError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM work.threads WHERE project_id = $1 AND slug = $2) AS "exists!""#,
            project_id,
            thread_slug,
        )
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn import_work_thread(
        &self,
        project_id: i64,
        thread: &ImportedThread,
    ) -> Result<ImportOutcome, WorkThreadError> {
        if thread.title.is_none() && thread.parse_error.is_none() {
            return Err(invalid(format!(
                "imported thread `{}` has neither a title nor a parse error",
                thread.slug
            )));
        }
        let mut tx = self.pool().begin().await?;
        if let Some(existing) = lock_thread(&mut tx, project_id, &thread.slug).await?
            && touched_in_mike(&mut tx, existing.id).await?
        {
            return Ok(ImportOutcome::LeftAlone(
                "it was changed in mike after it was imported, or was created in mike".into(),
            ));
        }
        let (thread_id, inserted) = upsert_identity(&mut tx, project_id, thread).await?;
        if let Some(state) = &thread.state {
            upsert_state(&mut tx, thread_id, state).await?;
        }
        if let Some(text) = &thread.acceptance_criteria {
            upsert_criteria(&mut tx, thread_id, &thread.source_path, text).await?;
        }
        upsert_log(&mut tx, thread_id, thread).await?;
        link_sessions(&mut tx, thread_id, thread).await?;
        tx.commit().await?;
        Ok(if inserted {
            ImportOutcome::Created
        } else {
            ImportOutcome::Updated
        })
    }

    pub async fn link_imported_dependencies(
        &self,
        project_id: i64,
        thread_slug: &str,
        depends_on: &[String],
    ) -> Result<Vec<String>, WorkThreadError> {
        let mut wanted: Vec<String> = depends_on
            .iter()
            .filter(|slug| slug.as_str() != thread_slug)
            .cloned()
            .collect();
        wanted.sort();
        wanted.dedup();
        sqlx::query!(
            "INSERT INTO work.thread_dependencies (thread_id, depends_on_thread_id)
             SELECT me.id, other.id
               FROM work.threads me
               JOIN work.threads other ON other.project_id = me.project_id AND other.slug = ANY($3)
              WHERE me.project_id = $1 AND me.slug = $2
             ON CONFLICT DO NOTHING",
            project_id,
            thread_slug,
            &wanted,
        )
        .execute(self.pool())
        .await?;
        Ok(sqlx::query_scalar!(
            r#"SELECT w.slug AS "slug!" FROM unnest($2::text[]) AS w(slug)
              WHERE NOT EXISTS (
                    SELECT 1 FROM work.threads t WHERE t.project_id = $1 AND t.slug = w.slug)
              ORDER BY w.slug"#,
            project_id,
            &wanted,
        )
        .fetch_all(self.pool())
        .await?)
    }
}

impl PersistHandle {
    pub async fn work_thread_project_roots(
        &self,
        project_id: i64,
    ) -> Result<Vec<String>, WorkThreadError> {
        Ok(sqlx::query_scalar!(
            "SELECT root_path FROM projects.locations WHERE project_id = $1 ORDER BY root_path",
            project_id,
        )
        .fetch_all(self.pool())
        .await?)
    }
}
