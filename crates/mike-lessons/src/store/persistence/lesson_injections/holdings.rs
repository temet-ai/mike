use chrono::{DateTime, Utc};

use super::super::PersistError;
use crate::lesson_cache::HeldRow;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn record_lesson_holdings(
        self,
        session_uuid: &str,
        route: &str,
        lessons: &[(i64, i32)],
    ) -> Result<u64, PersistError> {
        if lessons.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = lessons.iter().map(|(id, _)| *id).collect();
        let revisions: Vec<i32> = lessons.iter().map(|(_, revision)| *revision).collect();
        crate::metrics::time_db("record_lesson_holdings", async move {
            let result = sqlx::query!(
                "INSERT INTO memory.lesson_holdings (session_uuid, lesson_id, revision, route)
                 SELECT $1::text::uuid, l.id, u.revision, $2
                   FROM UNNEST($3::bigint[], $4::int[]) AS u(lesson_id, revision)
                   JOIN memory.lessons l ON l.id = u.lesson_id
                 ON CONFLICT (session_uuid, lesson_id) DO UPDATE SET
                    revision = EXCLUDED.revision,
                    route = EXCLUDED.route,
                    held_at = now()
                  WHERE memory.lesson_holdings.revision IS DISTINCT FROM EXCLUDED.revision",
                session_uuid,
                route,
                &ids,
                &revisions,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    pub async fn forget_lesson_holdings(self, session_uuid: &str) -> Result<u64, PersistError> {
        crate::metrics::time_db("forget_lesson_holdings", async move {
            let result = sqlx::query!(
                "DELETE FROM memory.lesson_holdings WHERE session_uuid = $1::text::uuid",
                session_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    pub async fn recent_lesson_holdings(
        self,
        since: DateTime<Utc>,
    ) -> Result<Vec<HeldRow>, PersistError> {
        crate::metrics::time_db("recent_lesson_holdings", async move {
            let rows = sqlx::query!(
                r#"SELECT session_uuid::text AS "session!", lesson_id, revision
                     FROM memory.lesson_holdings
                    WHERE held_at >= $1"#,
                since,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|row| HeldRow {
                    session: row.session,
                    lesson_id: row.lesson_id,
                    revision: row.revision,
                })
                .collect())
        })
        .await
    }

    pub async fn recent_client_sessions(
        self,
        since: DateTime<Utc>,
    ) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("recent_client_sessions", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT session_uuid::text AS "session!"
                     FROM memory.lesson_delivery_arms
                    WHERE COALESCE(last_call_at, last_delivery_at,
                                   CASE WHEN declared_at IS NULL THEN decided_at END) >= $1"#,
                since,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
