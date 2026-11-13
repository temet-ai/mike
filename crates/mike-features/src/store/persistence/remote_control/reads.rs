use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PersistError;
pub use crate::store::persistence::workspace::roster::ChannelSnapshotRow;

#[derive(Debug, Clone)]
pub struct WorkerStatusTransitionRow {
    pub channel_id: i64,
    pub from_status: Option<String>,
    pub to_status: String,
    pub observed_at: DateTime<Utc>,
    pub call_id: Option<i64>,
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn fetch_worker_status_history(
        self,
        session_uuid: Uuid,
        limit: i64,
    ) -> Result<Vec<WorkerStatusTransitionRow>, PersistError> {
        crate::metrics::time_db("remote_control_fetch_worker_status_history", async move {
            let rows = sqlx::query_as!(
                WorkerStatusTransitionRow,
                r#"
                SELECT
                    t.channel_id  AS "channel_id!",
                    t.from_status AS "from_status?",
                    t.to_status   AS "to_status!",
                    t.observed_at AS "observed_at!",
                    t.call_id     AS "call_id?"
                  FROM remote_control.worker_status_transitions t
                  JOIN remote_control.channels c ON c.id = t.channel_id
                 WHERE c.local_session_uuid = $1
                 ORDER BY t.observed_at DESC
                 LIMIT $2
                "#,
                session_uuid,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
