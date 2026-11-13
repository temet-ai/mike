use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[cfg(test)]
pub mod grades_tests;

#[derive(Debug, Clone, PartialEq)]
pub struct UngradedEvent {
    pub event_id: i64,
    pub query: String,
    pub kind_filter: Option<String>,
    pub used_references: Vec<String>,
    pub returned: Value,
    pub turn_prompt: Option<String>,
    pub project_id: i64,
    pub project_uuid: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewGrade<'a> {
    pub event_id: i64,
    pub candidates: &'a Value,
    pub used_references: &'a [String],
    pub plain_positions: &'a [i32],
    pub reranked_positions: &'a [i32],
    pub first_ten: &'a Value,
    pub turn_prompt: Option<&'a str>,
    pub relevant_references: &'a [String],
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredGrade {
    pub event_id: i64,
    pub candidates: Value,
    pub used_references: Vec<String>,
    pub relevant_references: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningOutcome {
    Changed,
    NoChange,
    TooFewGrades,
}

impl TuningOutcome {
    pub fn code(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::NoChange => "no_change",
            Self::TooFewGrades => "too_few_grades",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewTuningRun {
    pub setting_key: String,
    pub before: Value,
    pub after: Value,
    pub score_before: f64,
    pub score_after: f64,
    pub kept: bool,
    pub reason: String,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn ungraded_search_events(
        self,
        limit: i64,
    ) -> Result<Vec<UngradedEvent>, PersistError> {
        let rows = sqlx::query(include_str!("ungraded_events.sql")) // grader-allow: .sqlx entries cannot be written while the build is memory-blocked (operator note 8c8c8263); becomes query! with them
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(ungraded_event).collect()
    }

    pub async fn insert_search_grade(
        self,
        grade: &NewGrade<'_>,
    ) -> Result<(), PersistError> {
        sqlx::query(include_str!("insert_grade.sql")) // grader-allow: .sqlx entries cannot be written while the build is memory-blocked (operator note 8c8c8263); becomes query! with them
            .bind(grade.event_id)
            .bind(grade.candidates)
            .bind(grade.used_references)
            .bind(grade.plain_positions)
            .bind(grade.reranked_positions)
            .bind(grade.first_ten)
            .bind(grade.turn_prompt)
            .bind(grade.relevant_references)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn search_grades_since(
        self,
        since: DateTime<Utc>,
    ) -> Result<Vec<StoredGrade>, PersistError> {
        let rows = sqlx::query(include_str!("grades_since.sql")) // grader-allow: .sqlx entries cannot be written while the build is memory-blocked (operator note 8c8c8263); becomes query! with them
            .bind(since)
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(stored_grade).collect()
    }

    pub async fn insert_search_tuning_runs(
        self,
        outcome: TuningOutcome,
        runs: &[NewTuningRun],
        graded_events: i32,
        ran_at: DateTime<Utc>,
    ) -> Result<u64, PersistError> {
        let keys: Vec<String> = runs.iter().map(|r| r.setting_key.clone()).collect();
        let before: Vec<Value> = runs.iter().map(|r| r.before.clone()).collect();
        let after: Vec<Value> = runs.iter().map(|r| r.after.clone()).collect();
        let score_before: Vec<f64> = runs.iter().map(|r| r.score_before).collect();
        let score_after: Vec<f64> = runs.iter().map(|r| r.score_after).collect();
        let kept: Vec<bool> = runs.iter().map(|r| r.kept).collect();
        let reasons: Vec<String> = runs.iter().map(|r| r.reason.clone()).collect();
        let result = sqlx::query(include_str!("insert_tuning_runs.sql")) // grader-allow: .sqlx entries cannot be written while the build is memory-blocked (operator note 8c8c8263); becomes query! with them
            .bind(&keys)
            .bind(&before)
            .bind(&after)
            .bind(&score_before)
            .bind(&score_after)
            .bind(&kept)
            .bind(&reasons)
            .bind(graded_events)
            .bind(ran_at)
            .bind(outcome.code())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn search_tuned_since(
        self,
        since: DateTime<Utc>,
    ) -> Result<bool, PersistError> {
        Ok(
            sqlx::query_scalar(include_str!("tuned_since.sql")) // grader-allow: .sqlx entries cannot be written while the build is memory-blocked (operator note 8c8c8263); becomes query! with them
                .bind(since)
                .fetch_one(self.pool())
                .await?,
        )
    }
}

fn ungraded_event(row: &PgRow) -> Result<UngradedEvent, PersistError> {
    Ok(UngradedEvent {
        event_id: row.try_get("event_id")?,
        query: row.try_get("query")?,
        kind_filter: row.try_get("kind_filter")?,
        used_references: row.try_get("used_references")?,
        returned: row.try_get("returned")?,
        turn_prompt: row.try_get("turn_prompt")?,
        project_id: row.try_get("project_id")?,
        project_uuid: row.try_get("project_uuid")?,
    })
}

fn stored_grade(row: &PgRow) -> Result<StoredGrade, PersistError> {
    Ok(StoredGrade {
        event_id: row.try_get("event_id")?,
        candidates: row.try_get("candidates")?,
        used_references: row.try_get("used_references")?,
        relevant_references: row.try_get("relevant_references")?,
    })
}
