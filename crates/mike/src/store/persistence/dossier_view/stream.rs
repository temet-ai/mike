use chrono::{DateTime, Utc};

use super::super::PersistError;
use super::{DossierStreamRow, SessionHeaderRow};

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn fetch_session_header(
        self,
        session_db_id: i64,
    ) -> Result<Option<SessionHeaderRow>, PersistError> {
        crate::metrics::time_db("fetch_session_header", async move {
            let row = sqlx::query_as!(
                SessionHeaderRow,
                r#"SELECT s.id AS "session_db_id!", s.session_uuid::text AS "session_uuid!",
                          p.project_path AS "project_path?", s.first_seen AS "first_seen!"
                     FROM sessions.sessions s
                     LEFT JOIN projects.projects p ON p.id = s.project_id
                    WHERE s.id = $1"#,
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn fetch_stream_as_of(
        self,
        session_db_id: i64,
        at: DateTime<Utc>,
    ) -> Result<Vec<DossierStreamRow>, PersistError> {
        crate::metrics::time_db("fetch_stream_as_of", async move {
            let rows = sqlx::query_file_as!(
                DossierStreamRow,
                "src/store/persistence/dossier_view/stream.sql",
                session_db_id,
                at,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
