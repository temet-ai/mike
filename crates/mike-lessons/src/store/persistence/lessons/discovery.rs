use chrono::{DateTime, Duration, Utc};

use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;

const NOTHING_IDLE: &str = "lessons.no_idle_raw_lessons";
const LESSONS: &[RowChangedTable] = &[RowChangedTable::Lessons];

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn discover_idle_projects(self, idle_secs: u64) -> Result<Vec<i64>, PersistError> {
        let now = Utc::now();
        if self.is_fresh(NOTHING_IDLE, LESSONS, now) {
            return Ok(Vec::new());
        }
        let ticket = self.fresh_ticket();
        let pending = self.pending_raw_projects().await?;
        let idle = Duration::seconds(i64::from(u32::try_from(idle_secs).unwrap_or(u32::MAX)));
        let (ready, waiting): (Vec<_>, Vec<_>) = pending
            .into_iter()
            .partition(|(_, oldest)| *oldest < now - idle);
        if ready.is_empty() {
            let next = waiting.iter().map(|(_, oldest)| *oldest + idle).min();
            self.mark_fresh(NOTHING_IDLE, ticket, next);
        }
        Ok(ready.into_iter().map(|(project, _)| project).collect())
    }

    async fn pending_raw_projects(self) -> Result<Vec<(i64, DateTime<Utc>)>, PersistError> {
        crate::metrics::time_db("discover_idle_projects", async move {
            let rows = sqlx::query!(
                r#"SELECT l.project_id AS "project_id!", min(l.created_at) AS "oldest!"
                     FROM memory.lessons l
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code = 'raw'
                    WHERE l.embedded_at IS NULL
                      AND l.project_id IS NOT NULL
                      AND l.source_id NOT IN (SELECT id FROM memory.lesson_sources WHERE code = 'carry')
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.lesson_shape_findings f
                           WHERE f.lesson_id = l.id AND f.resolution IS NULL
                      )
                    GROUP BY l.project_id"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| (r.project_id, r.oldest)).collect())
        })
        .await
    }

    pub async fn discover_daily_projects(self) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("discover_daily_projects", async move {
            let ids = sqlx::query_scalar!(
                r#"SELECT DISTINCT l.project_id AS "project_id!"
                     FROM memory.lessons l
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code = 'daily'
                    WHERE l.project_id IS NOT NULL"#
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }
}
