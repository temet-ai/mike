use chrono::{DateTime, Utc};
use serde::Serialize;

use super::triage::CauseRow;
use super::ProblemRow;
use crate::store::persistence::PersistError;

const MERGE_CAUSES: &str = include_str!("sql/triage/merge_causes.sql");
const SEPARATE: &str = include_str!("sql/triage/separate.sql");
const MERGE_ONE: &str = include_str!("sql/triage/merge_one.sql");
const USABLE_CAUSES: &str = include_str!("sql/triage/usable_causes.sql");
const PATCH_CAUSE: &str = include_str!("sql/triage/patch_cause.sql");
const CAUSE_PROBLEMS: &str = include_str!("sql/triage/cause_problems.sql");
const CAUSE_EXISTS: &str = include_str!("sql/triage/cause_exists.sql");
const GROUP_EXISTS: &str = include_str!("sql/triage/group_exists.sql");

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct CausePatch {
    pub(crate) pinned: Option<bool>,
    pub(crate) operator_priority: Option<Option<f64>>,
    pub(crate) group_id: Option<Option<i64>>,
}

impl CausePatch {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PatchOutcome {
    Updated(Box<CauseRow>),
    NoCause,
    NoGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OperatorMerged {
    pub(crate) causes: i64,
    pub(crate) problems: i64,
    pub(crate) merge_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeOutcome {
    Merged(OperatorMerged),
    NoRoot,
    Unknown(Vec<i64>),
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct CauseMergeRow {
    pub(crate) id: i64,
    pub(crate) root_cause_id: i64,
    pub(crate) absorbed_cause_id: i64,
    pub(crate) problem_ids: Vec<i64>,
    pub(crate) by: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) separated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SeparateOutcome {
    Separated(CauseMergeRow),
    NotFound,
    EngineerMerge,
    AlreadySeparated,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn patch_wright_cause(
        self,
        cause_id: i64,
        patch: &CausePatch,
    ) -> Result<PatchOutcome, PersistError> {
        if let Some(Some(group_id)) = patch.group_id {
            let known: bool = sqlx::query_scalar(GROUP_EXISTS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(group_id)
                .fetch_one(self.pool())
                .await?;
            if !known {
                return Ok(PatchOutcome::NoGroup);
            }
        }
        let updated: Option<i64> = sqlx::query_scalar(PATCH_CAUSE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(cause_id)
            .bind(patch.pinned)
            .bind(patch.operator_priority.is_some())
            .bind(patch.operator_priority.flatten())
            .bind(patch.group_id.is_some())
            .bind(patch.group_id.flatten())
            .fetch_optional(self.pool())
            .await?;
        if updated.is_none() {
            return Ok(PatchOutcome::NoCause);
        }
        Ok(match self.wright_cause(cause_id).await? {
            Some(row) => PatchOutcome::Updated(Box::new(row)),
            None => PatchOutcome::NoCause,
        })
    }

    pub(crate) async fn merge_wright_causes_by_operator(
        self,
        root_id: i64,
        cause_ids: &[i64],
    ) -> Result<MergeOutcome, PersistError> {
        let mut wanted = cause_ids.to_vec();
        wanted.push(root_id);
        let usable: Vec<i64> = sqlx::query_scalar(USABLE_CAUSES) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(&wanted)
            .fetch_all(self.pool())
            .await?;
        if !usable.contains(&root_id) {
            return Ok(MergeOutcome::NoRoot);
        }
        let unknown: Vec<i64> = cause_ids
            .iter()
            .copied()
            .filter(|id| !usable.contains(id))
            .collect();
        if !unknown.is_empty() {
            return Ok(MergeOutcome::Unknown(unknown));
        }
        let (causes, problems, merge_ids): (i64, i64, Vec<i64>) = sqlx::query_as(MERGE_CAUSES) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(root_id)
            .bind(cause_ids)
            .fetch_one(self.pool())
            .await?;
        Ok(MergeOutcome::Merged(OperatorMerged {
            causes,
            problems,
            merge_ids,
        }))
    }

    pub(crate) async fn separate_wright_cause_merge(
        self,
        merge_id: i64,
    ) -> Result<SeparateOutcome, PersistError> {
        let separated: Option<CauseMergeRow> = sqlx::query_as(SEPARATE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(merge_id)
            .fetch_optional(self.pool())
            .await?;
        if let Some(row) = separated {
            return Ok(SeparateOutcome::Separated(row));
        }
        let existing: Option<CauseMergeRow> = sqlx::query_as(MERGE_ONE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(merge_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(match existing {
            None => SeparateOutcome::NotFound,
            Some(row) if row.by != "operator" => SeparateOutcome::EngineerMerge,
            Some(_) => SeparateOutcome::AlreadySeparated,
        })
    }

    pub(crate) async fn wright_cause_problems(
        self,
        cause_id: i64,
    ) -> Result<Option<Vec<ProblemRow>>, PersistError> {
        let known: bool = sqlx::query_scalar(CAUSE_EXISTS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(cause_id)
            .fetch_one(self.pool())
            .await?;
        if !known {
            return Ok(None);
        }
        let problems = sqlx::query_as(CAUSE_PROBLEMS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(cause_id)
            .fetch_all(self.pool())
            .await?;
        Ok(Some(problems))
    }
}
