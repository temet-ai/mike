use super::super::{PersistError, PersistHandle};
use super::{EntryClose, NewDossierEntry};

impl PersistHandle {
    pub async fn insert_dossier_entries(
        &self,
        entries: &[NewDossierEntry],
    ) -> Result<u64, PersistError> {
        if entries.is_empty() {
            return Ok(0);
        }
        let payload = serde_json::to_value(entries)?;
        crate::metrics::time_db("insert_dossier_entries", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/dossier_entries/insert_entries.sql",
                payload,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn close_dossier_entries(&self, closes: &[EntryClose]) -> Result<u64, PersistError> {
        if closes.is_empty() {
            return Ok(0);
        }
        let payload = serde_json::to_value(closes)?;
        crate::metrics::time_db("close_dossier_entries", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/dossier_entries/close_entries.sql",
                payload,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn supersede_directives(&self, session_ids: &[i64]) -> Result<u64, PersistError> {
        if session_ids.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("supersede_directives", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/dossier_entries/supersede_directives.sql",
                session_ids,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, sqlx::FromRow)]
pub struct EntryCountRow {
    pub kind: String,
    pub class: Option<String>,
    pub polarity: Option<String>,
    pub status: String,
    pub registry_coverage: Option<bool>,
    pub entries: i64,
}

impl PersistHandle {
    pub async fn count_dossier_entries_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<EntryCountRow>, PersistError> {
        crate::metrics::time_db("count_dossier_entries_since", async move {
            let rows = sqlx::query_file_as!(
                EntryCountRow,
                "src/store/persistence/dossier_entries/entry_counts.sql",
                since,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
