use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[cfg(test)]
mod grades_tests;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UngradedEvent {
    pub(crate) event_id: i64,
    pub(crate) query: String,
    pub(crate) kind_filter: Option<String>,
    pub(crate) used_references: Vec<String>,
    pub(crate) project_id: i64,
    pub(crate) project_uuid: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewGrade<'a> {
    pub(crate) event_id: i64,
    pub(crate) candidates: &'a Value,
    pub(crate) used_references: &'a [String],
    pub(crate) plain_positions: &'a [i32],
    pub(crate) reranked_positions: &'a [i32],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredGrade {
    pub(crate) event_id: i64,
    pub(crate) candidates: Value,
    pub(crate) used_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewTuningRun {
    pub(crate) setting_key: String,
    pub(crate) before: Value,
    pub(crate) after: Value,
    pub(crate) score_before: f64,
    pub(crate) score_after: f64,
    pub(crate) kept: bool,
    pub(crate) reason: String,
}

impl PersistHandle {
    pub(crate) async fn ungraded_search_events(
        &self,
        limit: i64,
    ) -> Result<Vec<UngradedEvent>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/search_grades/ungraded_events.sql",
            limit
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| UngradedEvent {
                event_id: row.event_id,
                query: row.query,
                kind_filter: row.kind_filter,
                used_references: row.used_references,
                project_id: row.project_id,
                project_uuid: row.project_uuid,
            })
            .collect())
    }

    pub(crate) async fn insert_search_grade(
        &self,
        grade: &NewGrade<'_>,
    ) -> Result<(), PersistError> {
        sqlx::query_file!(
            "src/store/persistence/search_grades/insert_grade.sql",
            grade.event_id,
            grade.candidates,
            grade.used_references,
            grade.plain_positions,
            grade.reranked_positions,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub(crate) async fn search_grades_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<StoredGrade>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/search_grades/grades_since.sql",
            since
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| StoredGrade {
                event_id: row.event_id,
                candidates: row.candidates,
                used_references: row.used_references,
            })
            .collect())
    }

    pub(crate) async fn insert_search_tuning_runs(
        &self,
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
        let result = sqlx::query_file!(
            "src/store/persistence/search_grades/insert_tuning_runs.sql",
            &keys,
            &before,
            &after,
            &score_before,
            &score_after,
            &kept,
            &reasons,
            graded_events,
            ran_at,
        )
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    pub(crate) async fn search_tuned_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<bool, PersistError> {
        Ok(
            sqlx::query_file_scalar!("src/store/persistence/search_grades/tuned_since.sql", since)
                .fetch_one(self.pool())
                .await?,
        )
    }
}
