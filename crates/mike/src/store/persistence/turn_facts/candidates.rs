#[cfg(test)]
#[path = "candidates_tests.rs"]
mod candidates_tests;

use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Copy)]
pub struct CandidateWindow {
    pub lookback_from: DateTime<Utc>,
    pub settled_before: DateTime<Utc>,
    pub limit: i64,
}

const BACKSTOP_MAX_ATTEMPTS: i32 = 10;

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn dossier_builder_enabled(self) -> Result<bool, PersistError> {
        crate::metrics::time_db("dossier_builder_enabled", async move {
            let enabled = sqlx::query_scalar!(
                "SELECT enabled FROM config.feature_toggles WHERE key = 'dossier_builder'"
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(enabled.unwrap_or(false))
        })
        .await
    }

    pub async fn sessions_for_turns(self, turn_ids: &[i64]) -> Result<Vec<i64>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("turn_facts_sessions_for_turns", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT DISTINCT t.session_id AS "session_id!"
                     FROM memory.operator_turns t
                    WHERE t.id = ANY($1)
                    ORDER BY t.session_id"#,
                turn_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn find_turn_facts_candidates(
        self,
        builder_version: &str,
        window: CandidateWindow,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("find_turn_facts_candidates", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/turn_facts/candidates.sql",
                window.lookback_from,
                builder_version,
                window.settled_before,
                window.limit,
                BACKSTOP_MAX_ATTEMPTS,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
