use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn session_last_request_at(
        &self,
        session_db_id: i64,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("session_last_request_at", async move {
            self.latest_request_at(session_db_id).await
        })
        .await
    }

    pub async fn latest_request_at(
        &self,
        session_db_id: i64,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("latest_request_at", async move {
            Ok(sqlx::query_scalar!(
                "SELECT created_at
                   FROM requests.requests
                  WHERE session_id = $1
                  ORDER BY created_at DESC, id DESC
                  LIMIT 1",
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn latest_request_at_many(
        &self,
        session_db_ids: &[i64],
    ) -> Result<HashMap<i64, DateTime<Utc>>, PersistError> {
        if session_db_ids.is_empty() {
            return Ok(HashMap::new());
        }
        crate::metrics::time_db("latest_request_at_many", async move {
            let rows = sqlx::query!(
                r#"SELECT s.id AS "session_id!", l.created_at AS "created_at!"
                     FROM unnest($1::bigint[]) AS s(id)
                    CROSS JOIN LATERAL (
                          SELECT r.created_at
                            FROM requests.requests r
                           WHERE r.session_id = s.id
                           ORDER BY r.created_at DESC, r.id DESC
                           LIMIT 1
                    ) l"#,
                session_db_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| (r.session_id, r.created_at))
                .collect())
        })
        .await
    }
}
