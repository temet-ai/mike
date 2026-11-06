use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct InterventionFeedRow {
    pub uuid: uuid::Uuid,
    pub session_uuid: uuid::Uuid,
    pub session_title: Option<String>,
    pub project_path: Option<String>,
    pub session_first_prompt: Option<String>,
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub question: Option<String>,
    pub arm: Option<String>,
    pub dispatched_at: DateTime<Utc>,
    pub outcome_label: Option<String>,
    pub label_source: Option<String>,
    pub delivered_request_id: Option<i64>,
    pub wake_state: Option<String>,
    pub delivery_channel: Option<String>,
    pub note_text: Option<String>,
    pub outcome_reaction: Option<String>,
}

impl PersistHandle {
    pub async fn guard_interventions_feed(
        &self,
        steerer_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<InterventionFeedRow>, PersistError> {
        crate::metrics::time_db("guard_interventions_feed", async move {
            Ok(sqlx::query_file_as!(
                InterventionFeedRow,
                "src/store/persistence/steering/promotion/interventions_feed.sql",
                steerer_id,
                limit,
                offset,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
