use chrono::{DateTime, Utc};

use super::super::PersistError;
use super::DossierTurnRow;

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn fetch_turns_as_of(
        self,
        session_db_id: i64,
        at: DateTime<Utc>,
    ) -> Result<Vec<DossierTurnRow>, PersistError> {
        crate::metrics::time_db("fetch_turns_as_of", async move {
            let rows = sqlx::query_file_as!(
                DossierTurnRow,
                "src/store/persistence/dossier_view/turns.sql",
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

pub use crate::store::persistence::operator_turns::turn_locator::TurnLocator;

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn fetch_turn_facts_by_id(
        self,
        turn_id: i64,
    ) -> Result<Option<crate::store::persistence::turn_facts::TurnFactsRow>, PersistError> {
        crate::metrics::time_db("fetch_turn_facts_by_id", async move {
            let row = sqlx::query_file_as!(
                crate::store::persistence::turn_facts::TurnFactsRow,
                "src/store/persistence/dossier_view/turn_facts_by_id.sql",
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}
