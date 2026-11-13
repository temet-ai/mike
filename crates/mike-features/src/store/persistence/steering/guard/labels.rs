use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct GuardLabelCandidate {
    pub uuid: uuid::Uuid,
    pub operator_turn_id: i64,
    pub session_id: i64,
    pub question: Option<String>,
    pub trigger_point: Option<String>,
    pub status: String,
    pub dispatched_at: DateTime<Utc>,
    pub verdict_json: Option<serde_json::Value>,
    pub is_last: bool,
    pub next_state: Option<String>,
    pub next_genuine_request_id: Option<i64>,
    pub next_genuine_after_secs: Option<i64>,
    pub next_genuine_text: Option<String>,
    pub reaction: Option<String>,
    pub live_verdict_id: Option<i64>,
    pub final_assistant_text: Option<String>,
    pub user_prompt: Option<String>,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn guard_label_candidates(
        self,
        steerer_id: i64,
        limit: i64,
        label_live_passes: bool,
    ) -> Result<Vec<GuardLabelCandidate>, PersistError> {
        crate::metrics::time_db("guard_label_candidates", async move {
            Ok(sqlx::query_file_as!(
                GuardLabelCandidate,
                "src/store/persistence/steering/guard/label_candidates.sql",
                steerer_id,
                limit,
                label_live_passes,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
