use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionToolRead {
    pub session_uuid: Uuid,
    pub tool_name: String,
    pub input: Option<serde_json::Value>,
}

impl PersistHandle {
    pub async fn session_tool_reads(
        &self,
        sessions: &[Uuid],
        tools: &[String],
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<SessionToolRead>, PersistError> {
        if sessions.is_empty() || tools.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("session_tool_reads", async move {
            let rows = sqlx::query_file_as!(
                SessionToolRead,
                "src/store/persistence/tool_uses/seat_reads.sql",
                sessions,
                tools,
                since,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
