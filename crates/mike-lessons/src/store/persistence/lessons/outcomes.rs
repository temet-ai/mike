use serde::Serialize;

use crate::store::persistence::PersistError;

pub use crate::lesson_cache::marks::LessonOutcome;

#[derive(Debug, Clone, PartialEq, Default, Serialize, sqlx::FromRow)]
pub struct LessonRetirement {
    pub retired_at: Option<chrono::DateTime<chrono::Utc>>,
    pub retired_kind: Option<String>,
    pub retired_reason: Option<String>,
    pub retired_by_lesson_id: Option<i64>,
}

const OUTCOME_COUNTS: &str = include_str!("outcome_counts.sql");
const RETIRE_BY_OUTCOME: &str = include_str!("retire_by_outcome.sql");
const SUPERSEDE_CONTRADICTED: &str = include_str!("supersede_contradicted.sql");
const RETIREMENT: &str = "SELECT retired_at, retired_kind, retired_reason, retired_by_lesson_id
                            FROM memory.lessons WHERE id = $1";

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn lesson_outcomes(
        self,
        lesson_ids: &[i64],
    ) -> Result<Vec<LessonOutcome>, PersistError> {
        if lesson_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("lesson_outcomes", async move {
            Ok(
                sqlx::query_as::<_, LessonOutcome>(OUTCOME_COUNTS) // grader-allow: reads memory.supplied_items (20261111170500), before its .sqlx entry is prepared
                    .bind(lesson_ids)
                    .fetch_all(&self.pool)
                    .await?,
            )
        })
        .await
    }

    pub async fn lesson_outcome(self, lesson_id: i64) -> Result<LessonOutcome, PersistError> {
        let found = self.lesson_outcomes(&[lesson_id]).await?;
        Ok(found.into_iter().next().unwrap_or(LessonOutcome {
            lesson_id,
            ..LessonOutcome::default()
        }))
    }

    pub async fn lesson_retirement(self, lesson_id: i64) -> Result<LessonRetirement, PersistError> {
        crate::metrics::time_db("lesson_retirement", async move {
            Ok(
                sqlx::query_as::<_, LessonRetirement>(RETIREMENT) // grader-allow: reads memory.lessons.retired_* (20261111170700), before its .sqlx entry is prepared
                    .bind(lesson_id)
                    .fetch_optional(&self.pool)
                    .await?
                    .unwrap_or_default(),
            )
        })
        .await
    }

    pub async fn retire_lessons_by_outcome(
        self,
        min_judged: i64,
        floor: f64,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("retire_lessons_by_outcome", async move {
            let ids: Vec<i64> = sqlx::query_scalar(RETIRE_BY_OUTCOME)
                .bind(min_judged)
                .bind(floor)
                .fetch_all(&self.pool)
                .await?;
            Ok(ids.len() as u64)
        })
        .await
    }

    pub async fn supersede_contradicted_lessons(self) -> Result<u64, PersistError> {
        crate::metrics::time_db("supersede_contradicted_lessons", async move {
            let ids: Vec<i64> = sqlx::query_scalar(SUPERSEDE_CONTRADICTED)
                .fetch_all(&self.pool)
                .await?;
            Ok(ids.len() as u64)
        })
        .await
    }
}
