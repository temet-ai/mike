use chrono::{DateTime, Utc};

use super::super::dossier_entries::DossierEntryRow;
use super::super::{PersistError, PersistHandle};
use super::LedgerAsOfRow;

impl PersistHandle {
    pub async fn fetch_ledger_as_of(
        &self,
        session_db_id: i64,
        at: DateTime<Utc>,
    ) -> Result<Vec<LedgerAsOfRow>, PersistError> {
        crate::metrics::time_db("fetch_ledger_as_of", async move {
            let rows = sqlx::query_file_as!(
                LedgerAsOfRow,
                "src/store/persistence/dossier_view/ledger.sql",
                session_db_id,
                at,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_entries_reconstructed(
        &self,
        session_db_id: i64,
        at: DateTime<Utc>,
        extractor_versions: &[String],
    ) -> Result<Vec<DossierEntryRow>, PersistError> {
        crate::metrics::time_db("list_entries_reconstructed", async move {
            let rows = sqlx::query_file_as!(
                DossierEntryRow,
                "src/store/persistence/dossier_view/ledger_reconstructed.sql",
                session_db_id,
                at,
                extractor_versions,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
