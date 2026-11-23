use crate::store::persistence::PersistError;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy)]
pub struct MeasureInput {
    pub run_id: i64,
    pub project_id: Option<i64>,
    pub split_subject_share: f32,
    pub grouping_distance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeasureRow {
    pub id: i64,
    pub measured_at: DateTime<Utc>,
    pub statements_current: i32,
    pub subjects: i32,
    pub subjects_shared: i32,
    pub conflicts_open: i32,
    pub guidance_chars: i64,
    pub misgrouped_share: f32,
    pub split_subject_share: f32,
    pub grouping_distance: f32,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn record_consolidation_measure(
        self,
        input: MeasureInput,
    ) -> Result<MeasureRow, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/consolidation/insert_measure.sql",
            input.run_id,
            input.project_id,
            input.split_subject_share,
            input.grouping_distance
        )
        .fetch_one(self.pool())
        .await?;
        Ok(MeasureRow {
            id: row.id,
            measured_at: row.measured_at,
            statements_current: row.statements_current,
            subjects: row.subjects,
            subjects_shared: row.subjects_shared,
            conflicts_open: row.conflicts_open,
            guidance_chars: row.guidance_chars,
            misgrouped_share: row.misgrouped_share,
            split_subject_share: row.split_subject_share,
            grouping_distance: row.grouping_distance,
        })
    }

    pub async fn latest_grouping_distance(
        self,
        project_id: Option<i64>,
    ) -> Result<Option<f32>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT grouping_distance FROM memory.knowledge_consolidation_measures
              WHERE project_id IS NOT DISTINCT FROM $1
              ORDER BY measured_at DESC, id DESC
              LIMIT 1",
            project_id
        )
        .fetch_optional(self.pool())
        .await?)
    }
}
