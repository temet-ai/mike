use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct SlowCommandRow {
    pub use_id: i64,
    pub seat: String,
    pub session: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub is_error: bool,
    pub in_background: bool,
    pub command: String,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn slow_commands(
        self,
        run_id: &str,
        since: DateTime<Utc>,
        slow_seconds: f64,
    ) -> Result<Vec<SlowCommandRow>, PersistError> {
        crate::metrics::time_db("launch_slow_commands", async move {
            let rows = sqlx::query_file_as!(
                SlowCommandRow,
                "src/store/persistence/fleet/slow_commands.sql",
                run_id,
                since,
                slow_seconds,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn record_introductions(
        self,
        run_id: &str,
        pairs: &[(i64, i64)],
    ) -> Result<Vec<i64>, PersistError> {
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let again: Vec<i64> = pairs.iter().map(|(again, _)| *again).collect();
        let first: Vec<i64> = pairs.iter().map(|(_, first)| *first).collect();
        crate::metrics::time_db("launch_record_introductions", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/fleet/record_introductions.sql",
                run_id,
                &again,
                &first,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| r.again_use_id).collect())
        })
        .await
    }
}
