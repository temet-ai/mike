use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn discover_idle_projects(&self, idle_secs: u64) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("discover_idle_projects", async move {
            let cutoff = chrono::Utc::now() - chrono::Duration::seconds(idle_secs as i64);
            let ids = sqlx::query_scalar!(
                r#"SELECT DISTINCT l.project_id AS "project_id!"
                     FROM memory.lessons l
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code = 'raw'
                    WHERE l.embedded_at IS NULL
                      AND l.project_id IS NOT NULL
                      AND l.source_id NOT IN (SELECT id FROM memory.lesson_sources WHERE code = 'carry')
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.lesson_shape_findings f
                           WHERE f.lesson_id = l.id AND f.resolution IS NULL
                      )
                      AND l.created_at < $1"#,
                cutoff,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }

    pub async fn discover_daily_projects(&self) -> Result<Vec<i64>, PersistError> {
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
