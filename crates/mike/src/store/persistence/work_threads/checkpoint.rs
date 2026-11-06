use serde::Serialize;
use uuid::Uuid;

use super::lock::{
    LockedThread, NewLogEntry, Tx, insert_log, insert_thread, link_session, lock_thread,
    touch_thread,
};
use super::meta::{MetaUpdate, apply_meta};
use super::reads::identity_by_id;
use super::validate::{contribution, first_line, invalid, non_blank, relative_paths, slug, tldr};
use super::{ThreadIdentity, ThreadState, WorkThreadError};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Copy, Default)]
pub struct StateFields<'a> {
    pub where_we_are: &'a str,
    pub next_up: &'a str,
    pub blockers: &'a str,
    pub open_questions: &'a str,
    pub decisions: &'a str,
    pub files: &'a [String],
}

#[derive(Debug, Clone, Copy)]
pub struct CheckpointInput<'a> {
    pub slug: &'a str,
    pub tldr: &'a str,
    pub state: StateFields<'a>,
    pub session_uuid: Option<Uuid>,
    pub commit_sha: Option<&'a str>,
    pub contribution: Option<&'a str>,
    pub meta: MetaUpdate<'a>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointOutcome {
    pub thread: ThreadIdentity,
    pub created: bool,
    pub state_replaced: bool,
    pub entry_ord: i32,
}

struct Checked<'a> {
    slug: &'a str,
    tldr: &'a str,
    files: Vec<String>,
    contribution: Option<&'a str>,
}

fn check<'a>(input: &CheckpointInput<'a>) -> Result<Checked<'a>, WorkThreadError> {
    Ok(Checked {
        slug: slug(input.slug)?,
        tldr: tldr(input.tldr)?,
        files: relative_paths("files", input.state.files)?,
        contribution: input.contribution.map(contribution).transpose()?,
    })
}

async fn resolve_or_create(
    tx: &mut Tx<'_>,
    project_id: i64,
    thread_slug: &str,
    title: Option<&str>,
) -> Result<(LockedThread, bool), WorkThreadError> {
    if let Some(thread) = lock_thread(tx, project_id, thread_slug).await? {
        return Ok((thread, false));
    }
    let title = title
        .map(|value| non_blank("title", value))
        .transpose()?
        .ok_or_else(|| {
            invalid(format!(
                "thread `{thread_slug}` does not exist in this project; send `title` to create it"
            ))
        })?;
    if let Some(thread) = insert_thread(tx, project_id, thread_slug, title).await? {
        return Ok((thread, true));
    }
    let thread = lock_thread(tx, project_id, thread_slug)
        .await?
        .ok_or_else(|| invalid(format!("thread `{thread_slug}` could not be created")))?;
    Ok((thread, false))
}

async fn keep_replaced_state(
    tx: &mut Tx<'_>,
    thread_id: i64,
    session: Option<Uuid>,
    commit_sha: Option<&str>,
) -> Result<bool, WorkThreadError> {
    let old = sqlx::query_as!(
        ThreadState,
        "SELECT tldr, where_we_are, next_up, blockers, open_questions, decisions, files, updated_at
           FROM work.thread_state WHERE thread_id = $1 FOR UPDATE",
        thread_id,
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(old) = old else {
        return Ok(false);
    };
    let data = serde_json::to_value(&old).map_err(crate::store::persistence::PersistError::from)?;
    let summary = format!(
        "State replaced; it was set {}",
        old.updated_at.format("%Y-%m-%d %H:%M UTC")
    );
    insert_log(
        tx,
        NewLogEntry {
            thread_id,
            kind: "state-replaced",
            session_uuid: session,
            commit_sha,
            summary: &summary,
            body: &old.tldr,
            data: Some(&data),
        },
    )
    .await?;
    Ok(true)
}

async fn write_state(
    tx: &mut Tx<'_>,
    thread_id: i64,
    checked: &Checked<'_>,
    state: &StateFields<'_>,
) -> Result<(), WorkThreadError> {
    sqlx::query!(
        "INSERT INTO work.thread_state
             (thread_id, tldr, where_we_are, next_up, blockers, open_questions, decisions, files)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (thread_id) DO UPDATE SET
            tldr = EXCLUDED.tldr,
            where_we_are = EXCLUDED.where_we_are,
            next_up = EXCLUDED.next_up,
            blockers = EXCLUDED.blockers,
            open_questions = EXCLUDED.open_questions,
            decisions = EXCLUDED.decisions,
            files = EXCLUDED.files,
            updated_at = now(),
            source_updated_at = NULL,
            embedding = NULL, embedding_model = NULL, embedded_at = NULL",
        thread_id,
        checked.tldr,
        state.where_we_are.trim(),
        state.next_up.trim(),
        state.blockers.trim(),
        state.open_questions.trim(),
        state.decisions.trim(),
        &checked.files,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

impl PersistHandle {
    pub async fn checkpoint_work_thread(
        &self,
        project_id: i64,
        input: &CheckpointInput<'_>,
    ) -> Result<CheckpointOutcome, WorkThreadError> {
        let checked = check(input)?;
        let mut tx = self.pool().begin().await?;
        let (thread, created) =
            resolve_or_create(&mut tx, project_id, checked.slug, input.meta.title).await?;
        apply_meta(&mut tx, project_id, &thread, checked.slug, &input.meta).await?;
        let state_replaced =
            keep_replaced_state(&mut tx, thread.id, input.session_uuid, input.commit_sha).await?;
        write_state(&mut tx, thread.id, &checked, &input.state).await?;
        let entry = insert_log(
            &mut tx,
            NewLogEntry {
                thread_id: thread.id,
                kind: "checkpoint",
                session_uuid: input.session_uuid,
                commit_sha: input.commit_sha,
                summary: &first_line(checked.tldr),
                body: checked.tldr,
                data: None,
            },
        )
        .await?;
        touch_thread(&mut tx, thread.id, input.session_uuid).await?;
        if let Some(session) = input.session_uuid {
            link_session(&mut tx, thread.id, session, checked.contribution).await?;
        }
        tx.commit().await?;
        let thread = identity_by_id(self.pool(), thread.id).await?;
        Ok(CheckpointOutcome {
            thread,
            created,
            state_replaced,
            entry_ord: entry.ord,
        })
    }
}
