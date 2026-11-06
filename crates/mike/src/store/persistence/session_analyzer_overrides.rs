use std::collections::HashMap;

use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn set_session_analyzer_override(
        &self,
        session_uuid: uuid::Uuid,
        analyzer_name: &str,
        enabled: bool,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("set_session_analyzer_override", async move {
            let result = sqlx::query!(
                r#"INSERT INTO sessions.session_analyzer_overrides
                       (session_id, analyzer_name, enabled, updated_at)
                   SELECT s.id, $2, $3, now()
                     FROM sessions.sessions s
                    WHERE s.session_uuid = $1
                   ON CONFLICT (session_id, analyzer_name) DO UPDATE
                       SET enabled    = excluded.enabled,
                           updated_at = now()"#,
                session_uuid,
                analyzer_name,
                enabled,
            )
            .execute(&self.pool)
            .await?;

            if result.rows_affected() == 0 {
                return Err(PersistError::DataInvariant(format!(
                    "set_session_analyzer_override: session_uuid {session_uuid} not found"
                )));
            }
            Ok(())
        })
        .await
    }

    pub async fn get_session_analyzer_overrides(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<HashMap<String, bool>, PersistError> {
        crate::metrics::time_db("get_session_analyzer_overrides", async move {
            let rows = sqlx::query!(
                r#"SELECT sao.analyzer_name, sao.enabled
                     FROM sessions.session_analyzer_overrides sao
                     JOIN sessions.sessions s ON s.id = sao.session_id
                    WHERE s.session_uuid = $1"#,
                session_uuid,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| (r.analyzer_name, r.enabled))
                .collect())
        })
        .await
    }
}
