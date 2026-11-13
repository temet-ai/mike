use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SteeringFactsWrite {
    pub operator_turn_id: i64,
    pub wait_backing: String,
    pub wait_backing_refs: serde_json::Value,
    pub observed_after_change: Option<bool>,
    pub last_change_use_id: Option<i64>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub last_observation_use_id: Option<i64>,
    pub last_observation_at: Option<DateTime<Utc>>,
    pub observation_kind: Option<String>,
    pub steering_facts_version: i16,
}

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn write_steering_facts(
        self,
        rows: &[SteeringFactsWrite],
    ) -> Result<u64, PersistError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let payload = serde_json::to_value(rows)?;
        crate::metrics::time_db("write_steering_facts", async move {
            Ok(
                sqlx::query_file!("src/store/persistence/steering_facts/write.sql", payload,)
                    .execute(&self.pool)
                    .await?
                    .rows_affected(),
            )
        })
        .await
    }
}
