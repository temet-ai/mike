use crate::store::persistence::PersistError;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SubjectSummary {
    pub id: i64,
    pub project_id: Option<i64>,
    pub title: String,
    pub text: String,
    pub members: i32,
    pub carriers: i32,
    pub changed_at: Option<DateTime<Utc>>,
    pub judged_at: Option<DateTime<Utc>>,
    pub newest_decision_at: Option<DateTime<Utc>>,
    pub newest_verdict: Option<String>,
    pub newest_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DecisionFilter<'a> {
    pub verdicts: Option<&'a [String]>,
    pub open_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionCursor {
    pub created_at: DateTime<Utc>,
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DecisionRow {
    pub id: i64,
    pub run_id: i64,
    pub project_id: Option<i64>,
    pub subject_id: Option<i64>,
    pub subject_title: Option<String>,
    pub statement_id: Option<i64>,
    pub item_id: Option<i64>,
    pub carrier_reference: String,
    pub verdict: String,
    pub counterpart_statement_id: Option<i64>,
    pub counterpart_reference: Option<String>,
    pub text_before: String,
    pub text_after: Option<String>,
    pub reason: String,
    pub evidence: Value,
    pub model: String,
    pub application: String,
    pub applied_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeasureSeriesRow {
    pub id: i64,
    pub run_id: i64,
    pub project_id: Option<i64>,
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
    pub async fn list_knowledge_subjects(
        self,
        project_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SubjectSummary>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/list_subjects.sql",
            project_id,
            limit,
            offset
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| SubjectSummary {
                id: row.id,
                project_id: row.project_id,
                title: row.title,
                text: row.text,
                members: row.members,
                carriers: row.carriers,
                changed_at: row.changed_at,
                judged_at: row.judged_at,
                newest_decision_at: row.newest_decision_at,
                newest_verdict: row.newest_verdict,
                newest_reason: row.newest_reason,
            })
            .collect())
    }

    pub async fn list_consolidation_decisions(
        self,
        project_id: i64,
        subject_id: Option<i64>,
        before: Option<DecisionCursor>,
        limit: i64,
    ) -> Result<Vec<DecisionRow>, PersistError> {
        self.list_consolidation_decisions_filtered(
            project_id,
            subject_id,
            before,
            limit,
            DecisionFilter::default(),
        )
        .await
    }

    pub async fn list_consolidation_decisions_filtered(
        self,
        project_id: i64,
        subject_id: Option<i64>,
        before: Option<DecisionCursor>,
        limit: i64,
        filter: DecisionFilter<'_>,
    ) -> Result<Vec<DecisionRow>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/list_decisions.sql",
            project_id,
            subject_id,
            before.map(|c| c.created_at),
            before.map_or(0, |c| c.id),
            limit,
            filter.verdicts,
            filter.open_only
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| DecisionRow {
                id: row.id,
                run_id: row.run_id,
                project_id: row.project_id,
                subject_id: row.subject_id,
                subject_title: row.subject_title,
                statement_id: row.statement_id,
                item_id: row.item_id,
                carrier_reference: row.carrier_reference,
                verdict: row.verdict,
                counterpart_statement_id: row.counterpart_statement_id,
                counterpart_reference: row.counterpart_reference,
                text_before: row.text_before,
                text_after: row.text_after,
                reason: row.reason,
                evidence: row.evidence,
                model: row.model,
                application: row.application,
                applied_at: row.applied_at,
                error: row.error,
                created_at: row.created_at,
            })
            .collect())
    }

    pub async fn list_consolidation_measures(
        self,
        project_id: Option<i64>,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<MeasureSeriesRow>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/list_measures.sql",
            project_id,
            since,
            limit
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MeasureSeriesRow {
                id: row.id,
                run_id: row.run_id,
                project_id: row.project_id,
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
            .collect())
    }

    pub async fn get_consolidation_decision(
        self,
        decision_id: i64,
    ) -> Result<Option<DecisionRow>, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/consolidation/get_decision.sql",
            decision_id
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|row| DecisionRow {
            id: row.id,
            run_id: row.run_id,
            project_id: row.project_id,
            subject_id: row.subject_id,
            subject_title: row.subject_title,
            statement_id: row.statement_id,
            item_id: row.item_id,
            carrier_reference: row.carrier_reference,
            verdict: row.verdict,
            counterpart_statement_id: row.counterpart_statement_id,
            counterpart_reference: row.counterpart_reference,
            text_before: row.text_before,
            text_after: row.text_after,
            reason: row.reason,
            evidence: row.evidence,
            model: row.model,
            application: row.application,
            applied_at: row.applied_at,
            error: row.error,
            created_at: row.created_at,
        }))
    }

    pub async fn get_knowledge_subject(
        self,
        subject_id: i64,
    ) -> Result<Option<SubjectSummary>, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/consolidation/get_subject.sql",
            subject_id
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|row| SubjectSummary {
            id: row.id,
            project_id: row.project_id,
            title: row.title,
            text: row.text,
            members: row.members,
            carriers: row.carriers,
            changed_at: row.changed_at,
            judged_at: row.judged_at,
            newest_decision_at: row.newest_decision_at,
            newest_verdict: row.newest_verdict,
            newest_reason: row.newest_reason,
        }))
    }
}
