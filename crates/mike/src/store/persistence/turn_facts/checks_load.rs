use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct BashUseRecord {
    pub turn_id: i64,
    pub tool_use_id: String,
    pub request_id: i64,
    pub at: DateTime<Utc>,
    pub command: String,
    pub result_content: Option<serde_json::Value>,
    pub result_is_error: Option<bool>,
    pub work_item_id: Option<i64>,
    pub work_item_state: Option<String>,
    pub work_item_exit_code: Option<i32>,
    pub work_item_finished_at: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub async fn load_bash_uses(
        &self,
        turn_ids: &[i64],
    ) -> Result<Vec<BashUseRecord>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("load_turn_bash_uses", async move {
            Ok(sqlx::query_file_as!(
                BashUseRecord,
                "src/store/persistence/turn_facts/checks_load.sql",
                turn_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
