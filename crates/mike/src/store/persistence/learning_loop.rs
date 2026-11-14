use chrono::{DateTime, Utc};

use super::PersistError;

pub const RELATIONS_WINDOW: i64 = 1000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoopCounts {
    pub requests: i64,
    pub requests_at: Option<DateTime<Utc>>,
    pub operator_turns: i64,
    pub operator_turns_at: Option<DateTime<Utc>>,
    pub turn_verdicts: i64,
    pub turn_verdicts_at: Option<DateTime<Utc>>,
    pub session_verdicts: i64,
    pub session_verdicts_at: Option<DateTime<Utc>>,
    pub lessons: i64,
    pub lessons_at: Option<DateTime<Utc>>,
    pub lesson_relations: i64,
    pub lesson_relations_at: Option<DateTime<Utc>>,
    pub consolidation_runs: i64,
    pub consolidation_runs_at: Option<DateTime<Utc>>,
    pub supplied_items: i64,
    pub supplied_items_at: Option<DateTime<Utc>>,
    pub supply_verdicts: i64,
    pub supply_verdicts_at: Option<DateTime<Utc>>,
    pub search_grades: i64,
    pub search_grades_at: Option<DateTime<Utc>>,
    pub memory_reviews: i64,
    pub memory_reviews_at: Option<DateTime<Utc>>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub async fn learning_loop_counts(
        self,
        since: DateTime<Utc>,
    ) -> Result<LoopCounts, PersistError> {
        crate::metrics::time_db("learning_loop_counts", async move {
            Ok(sqlx::query_file_as!(
                LoopCounts,
                "src/store/persistence/learning_loop.sql",
                since,
                RELATIONS_WINDOW
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
