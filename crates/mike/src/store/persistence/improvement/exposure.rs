use crate::store::persistence::{PersistError, PersistHandle};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LessonExposureRow {
    pub lesson_id: i64,
    pub recorded_injections: i64,
    pub unattributed_injections: i64,
    pub exposed_turns: i64,
    pub judged_turns: i64,
    pub corrected_turns: i64,
    pub accepted_turns: i64,
    pub delivered_turns: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub usage_requests: i64,
    pub requests: i64,
    pub tool_calls: i64,
    pub whole_turn_failed_checks: i64,
}

impl PersistHandle {
    pub async fn list_lesson_exposures(
        &self,
        project_path: Option<&str>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<LessonExposureRow>, PersistError> {
        let rows = match project_path {
            Some(path) => {
                sqlx::query_file_as!(
                    LessonExposureRow,
                    "src/store/persistence/improvement/lesson_exposures_for_project.sql",
                    path,
                    since,
                    until,
                )
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_file_as!(
                    LessonExposureRow,
                    "src/store/persistence/improvement/lesson_exposures.sql",
                    since,
                    until,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows)
    }
}
