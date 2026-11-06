use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ArmRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub question: Option<String>,
    pub arm: Option<String>,
    pub holdback_draw: Option<f64>,
    pub arm_anchor_turn_id: Option<i64>,
    pub due_at: Option<DateTime<Utc>>,
    pub reach_at_due: Option<String>,
    pub dispatched_at: DateTime<Utc>,
    pub outcome_label: Option<String>,
    pub label_source: Option<String>,
    pub carrier_request_id: Option<i64>,
    pub scored_turn_id: Option<i64>,
    pub scored_reaction: Option<String>,
    pub scored_is_last: bool,
    pub delivered: bool,
    pub wake_state: Option<String>,
}

impl PersistHandle {
    pub async fn fetch_arm_rows(
        &self,
        steerer_id: i64,
        question: Option<&str>,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<ArmRow>, PersistError> {
        crate::metrics::time_db("fetch_arm_rows", async move {
            Ok(match question {
                Some(question) => {
                    sqlx::query_file_as!(
                        ArmRow,
                        "src/store/persistence/steering/promotion/arm_rows_for_question.sql",
                        steerer_id,
                        question,
                        as_of,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        ArmRow,
                        "src/store/persistence/steering/promotion/arm_rows.sql",
                        steerer_id,
                        as_of,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            })
        })
        .await
    }
}
