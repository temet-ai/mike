use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use uuid::Uuid;

use super::{NewSessionNote, SessionNoteRow, SessionNoteUpdate};
use crate::store::persistence::launch_repo::LaunchDb;
use crate::store::persistence::{PersistError, PersistHandle};

pub type RuleFuture<'a> = Pin<Box<dyn Future<Output = Result<(), PersistError>> + Send + 'a>>;
pub type AfterFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

pub trait NoteRules: Send + Sync {
    fn validate<'a>(
        &'a self,
        connection: &'a mut sqlx::PgConnection,
        row: &'a SessionNoteRow,
    ) -> RuleFuture<'a>;

    fn protect_update<'a>(
        &'a self,
        connection: &'a mut sqlx::PgConnection,
        note: Uuid,
        update: SessionNoteUpdate<'a>,
    ) -> RuleFuture<'a>;

    fn protect_delete<'a>(
        &'a self,
        connection: &'a mut sqlx::PgConnection,
        note: Uuid,
    ) -> RuleFuture<'a>;

    fn after_write<'a>(
        &'a self,
        persist: &'a PersistHandle,
        row: &'a SessionNoteRow,
    ) -> AfterFuture<'a>;
}

pub struct NoRules;

impl NoteRules for NoRules {
    fn validate<'a>(
        &'a self,
        _: &'a mut sqlx::PgConnection,
        _: &'a SessionNoteRow,
    ) -> RuleFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn protect_update<'a>(
        &'a self,
        _: &'a mut sqlx::PgConnection,
        _: Uuid,
        _: SessionNoteUpdate<'a>,
    ) -> RuleFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn protect_delete<'a>(&'a self, _: &'a mut sqlx::PgConnection, _: Uuid) -> RuleFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn after_write<'a>(&'a self, _: &'a PersistHandle, _: &'a SessionNoteRow) -> AfterFuture<'a> {
        Box::pin(async {})
    }
}

static RULES: OnceLock<Arc<dyn NoteRules>> = OnceLock::new();

pub fn install_rules(rules: Arc<dyn NoteRules>) {
    let _ = RULES.set(rules);
}

fn installed() -> Arc<dyn NoteRules> {
    RULES.get().cloned().unwrap_or_else(|| Arc::new(NoRules))
}

impl LaunchDb<'_> {
    pub async fn create_session_note(
        self,
        session_uuid: Uuid,
        note: NewSessionNote<'_>,
    ) -> Result<SessionNoteRow, PersistError> {
        let rules = installed();
        self.create_session_note_with(rules.as_ref(), session_uuid, note)
            .await
    }

    pub async fn update_session_note(
        self,
        note_uuid: Uuid,
        update: SessionNoteUpdate<'_>,
    ) -> Result<Option<SessionNoteRow>, PersistError> {
        let rules = installed();
        self.update_session_note_with(rules.as_ref(), note_uuid, update)
            .await
    }

    pub async fn delete_session_note(self, note_uuid: Uuid) -> Result<bool, PersistError> {
        let rules = installed();
        self.delete_session_note_with(rules.as_ref(), note_uuid)
            .await
    }

    pub async fn create_session_note_with(
        self,
        rules: &dyn NoteRules,
        session_uuid: Uuid,
        note: NewSessionNote<'_>,
    ) -> Result<SessionNoteRow, PersistError> {
        crate::metrics::time_db("session_notes_create", async move {
            let mut transaction = self.pool.begin().await?;
            let row = sqlx::query_as!(
                SessionNoteRow,
                r#"INSERT INTO workspace.session_notes (session_id, kind, title, body, status, tags, data)
                   SELECT s.id, COALESCE($2, 'note'), $3, COALESCE($4, ''), $5,
                          COALESCE($6::text[], '{}'), COALESCE($7, '{}'::jsonb)
                     FROM sessions.sessions s
                    WHERE s.session_uuid = $1
               RETURNING note_uuid, kind, title, body, status, tags, data, created_at, updated_at"#,
                session_uuid,
                note.kind,
                note.title,
                note.body,
                note.status,
                note.tags as Option<&[String]>,
                note.data as Option<&serde_json::Value>,
            )
            .fetch_optional(&mut *transaction)
            .await?;
            let row = row.ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "create_session_note: session_uuid {session_uuid} not found"
                ))
            })?;
            rules.validate(&mut transaction, &row).await?;
            transaction.commit().await?;
            rules.after_write(&self, &row).await;
            Ok(row)
        })
        .await
    }

    pub async fn update_session_note_with(
        self,
        rules: &dyn NoteRules,
        note_uuid: Uuid,
        update: SessionNoteUpdate<'_>,
    ) -> Result<Option<SessionNoteRow>, PersistError> {
        crate::metrics::time_db("session_notes_update", async move {
            let mut transaction = self.pool.begin().await?;
            rules
                .protect_update(&mut transaction, note_uuid, update)
                .await?;
            let row = sqlx::query_as!(
                SessionNoteRow,
                r#"UPDATE workspace.session_notes
                      SET kind       = COALESCE($2, kind),
                          title      = COALESCE($3, title),
                          body       = COALESCE($4, body),
                          status     = COALESCE($5, status),
                          tags       = COALESCE($6::text[], tags),
                          data       = COALESCE($7, data),
                          updated_at = now()
                    WHERE note_uuid = $1
                RETURNING note_uuid, kind, title, body, status, tags, data, created_at, updated_at"#,
                note_uuid,
                update.kind,
                update.title,
                update.body,
                update.status,
                update.tags as Option<&[String]>,
                update.data as Option<&serde_json::Value>,
            )
            .fetch_optional(&mut *transaction)
            .await?;
            if let Some(row) = &row {
                rules.validate(&mut transaction, row).await?;
            }
            transaction.commit().await?;
            if let Some(row) = &row {
                rules.after_write(&self, row).await;
            }
            Ok(row)
        })
        .await
    }

    pub async fn delete_session_note_with(
        self,
        rules: &dyn NoteRules,
        note_uuid: Uuid,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("session_notes_delete", async move {
            let mut transaction = self.pool.begin().await?;
            rules.protect_delete(&mut transaction, note_uuid).await?;
            let result = sqlx::query!(
                "DELETE FROM workspace.session_notes WHERE note_uuid = $1",
                note_uuid,
            )
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }
}
