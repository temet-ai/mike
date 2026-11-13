use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn backfill_sessions_with_prompts(
        self,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("backfill_sessions_with_prompts", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT DISTINCT r.session_id AS "session_id!"
                     FROM requests.requests r
                     JOIN requests.turn_types tt ON tt.id = r.turn_type_id AND tt.code = 'user_prompt'
                     JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                                                 AND o.code IN ('primary', 'primary_first')
                    WHERE r.created_at >= $1
                      AND r.created_at < $2
                    ORDER BY r.session_id"#,
                from,
                until,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn backfill_sessions_with_turns(
        self,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("backfill_sessions_with_turns", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT DISTINCT t.session_id AS "session_id!"
                     FROM memory.operator_turns t
                    WHERE t.created_at >= $1
                      AND t.created_at < $2
                    ORDER BY t.session_id"#,
                from,
                until,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn count_primary_requests_with_agent_id(
        self,
        since: DateTime<Utc>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_primary_requests_with_agent_id", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT count(*) AS "count!"
                     FROM requests.requests r
                     JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                                                 AND o.code IN ('primary', 'primary_first')
                    WHERE r.created_at >= $1
                      AND r.claude_code_agent_id IS NOT NULL"#,
                since,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
