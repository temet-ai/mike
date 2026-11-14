use super::{NewSessionNote, SessionNoteRow, SessionNoteUpdate};
use crate::store::persistence::PersistError;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn create_session_note(
        self,
        session_uuid: uuid::Uuid,
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
            super::obligation::validate(&mut transaction, &row).await?;
            transaction.commit().await?;
            self.carry_after_write(&row).await;
            Ok(row)
        })
        .await
    }

    pub async fn update_session_note(
        self,
        note_uuid: uuid::Uuid,
        update: SessionNoteUpdate<'_>,
    ) -> Result<Option<SessionNoteRow>, PersistError> {
        crate::metrics::time_db("session_notes_update", async move {
            let mut transaction = self.pool.begin().await?;
            super::obligation::protect_update(&mut transaction, note_uuid, update).await?;
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
                super::obligation::validate(&mut transaction, row).await?;
            }
            transaction.commit().await?;
            if let Some(row) = &row {
                self.carry_after_write(row).await;
            }
            Ok(row)
        })
        .await
    }

    async fn carry_after_write(self, row: &SessionNoteRow) {
        crate::launch::fleet::carry::after_note(&self, row.note_uuid, &row.tags, &row.data).await;
        crate::launch::fleet::ruling_notice::after_note(&self, row).await;
    }

    pub async fn delete_session_note(self, note_uuid: uuid::Uuid) -> Result<bool, PersistError> {
        crate::metrics::time_db("session_notes_delete", async move {
            let mut transaction = self.pool.begin().await?;
            super::obligation::protect_delete(&mut transaction, note_uuid).await?;
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
