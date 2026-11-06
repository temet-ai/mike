use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::lock::{NewLogEntry, insert_log, link_session, require_thread, touch_thread};
use super::validate::{first_line, kind, non_blank, slug};
use super::{HistoryEntry, WorkThreadError};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Copy)]
pub struct EntryInput<'a> {
    pub slug: &'a str,
    pub kind: &'a str,
    pub body: &'a str,
    pub summary: Option<&'a str>,
    pub session_uuid: Option<Uuid>,
    pub commit_sha: Option<&'a str>,
    pub data: Option<&'a Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryOutcome {
    pub entry: HistoryEntry,
}

impl PersistHandle {
    pub async fn add_work_thread_entry(
        &self,
        project_id: i64,
        input: &EntryInput<'_>,
    ) -> Result<EntryOutcome, WorkThreadError> {
        let thread_slug = slug(input.slug)?;
        let entry_kind = kind(input.kind)?;
        let body = non_blank("body", input.body)?;
        let summary = match input.summary.map(str::trim).filter(|s| !s.is_empty()) {
            Some(summary) => summary.to_owned(),
            None => first_line(body),
        };
        let mut tx = self.pool().begin().await?;
        let thread = require_thread(&mut tx, project_id, thread_slug).await?;
        let entry = insert_log(
            &mut tx,
            NewLogEntry {
                thread_id: thread.id,
                kind: entry_kind,
                session_uuid: input.session_uuid,
                commit_sha: input.commit_sha.map(str::trim).filter(|s| !s.is_empty()),
                summary: &summary,
                body,
                data: input.data,
            },
        )
        .await?;
        touch_thread(&mut tx, thread.id, input.session_uuid).await?;
        if let Some(session) = input.session_uuid {
            link_session(&mut tx, thread.id, session, None).await?;
        }
        tx.commit().await?;
        Ok(EntryOutcome { entry })
    }
}
