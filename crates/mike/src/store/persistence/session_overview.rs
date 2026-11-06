use chrono::{DateTime, Utc};

use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn set_session_overview(
        &self,
        session_uuid: uuid::Uuid,
        overview: &str,
        generated_at: Option<DateTime<Utc>>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_session_overview", async move {
            let result = sqlx::query!(
                "UPDATE sessions.sessions
                    SET overview = $2,
                        overview_generated_at = $3
                  WHERE session_uuid = $1",
                session_uuid,
                overview,
                generated_at,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub(crate) async fn fetch_session_overview_text(
        &self,
        session_db_id: i64,
    ) -> Result<Option<(String, Option<DateTime<Utc>>)>, PersistError> {
        crate::metrics::time_db("fetch_session_overview_text", async move {
            let row = sqlx::query!(
                r#"SELECT overview AS "overview?",
                          overview_generated_at AS "overview_generated_at?"
                     FROM sessions.sessions
                    WHERE id = $1"#,
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.and_then(|r| r.overview.map(|o| (o, r.overview_generated_at))))
        })
        .await
    }

    pub(crate) async fn set_session_overview_generated(
        &self,
        session_uuid: uuid::Uuid,
        overview: &str,
        generated_at: DateTime<Utc>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_session_overview_generated", async move {
            let result = sqlx::query!(
                "UPDATE sessions.sessions
                    SET overview = $2,
                        overview_generated_at = $3
                  WHERE session_uuid = $1
                    AND (overview IS NULL OR overview_generated_at IS NOT NULL)",
                session_uuid,
                overview,
                generated_at,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }
}
