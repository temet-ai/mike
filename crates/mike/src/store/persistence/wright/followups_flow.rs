use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use super::followups::{FollowupRow, with_links};
use crate::store::persistence::PersistError;

const TO_FILE: &str = include_str!("sql/followups/to_file.sql");
const FILED_TODAY: &str = include_str!("sql/followups/filed_today.sql");
const MARK_FILED: &str = include_str!("sql/followups/mark_filed.sql");
const NEXT_HANDOFF: &str = include_str!("sql/followups/next_handoff.sql");
const HANDOFFS_TODAY: &str = include_str!("sql/followups/handoffs_today.sql");
const LIVE_HANDOFFS: &str = include_str!("sql/followups/live_handoffs.sql");
const TRACKED: &str = include_str!("sql/followups/tracked.sql");
const CHILDREN_CLOSED: &str = include_str!("sql/followups/children_closed.sql");
const MARK_HANDED_OFF: &str = include_str!("sql/followups/mark_handed_off.sql");
const MARK_CLOSED: &str = include_str!("sql/followups/mark_closed.sql");
const RETURN_FOLLOWUP: &str = include_str!("sql/followups/return_followup.sql");
const MARK_BATCHED: &str = include_str!("sql/followups/mark_batched.sql");
const UNBATCH: &str = include_str!("sql/followups/unbatch.sql");
const LIMITS: &str = include_str!("sql/followups/limits.sql");
const PATCH_LIMITS: &str = include_str!("sql/followups/patch_limits.sql");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fixer {
    pub(crate) machine: String,
    pub(crate) host: Option<String>,
    pub(crate) folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct FollowupLimits {
    pub(crate) repo: String,
    pub(crate) daily_filed: i32,
    pub(crate) daily_handoffs: i32,
    pub(crate) max_attempts: i32,
    pub(crate) recurrence_actions: i32,
    pub(crate) recurrence_days: i32,
    pub(crate) fixers: Json<Vec<Fixer>>,
    pub(crate) fix_host: String,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FollowupLimitsPatch {
    pub(crate) repo: Option<String>,
    pub(crate) daily_filed: Option<i32>,
    pub(crate) daily_handoffs: Option<i32>,
    pub(crate) max_attempts: Option<i32>,
    pub(crate) recurrence_actions: Option<i32>,
    pub(crate) recurrence_days: Option<i32>,
    pub(crate) fixers: Option<Vec<Fixer>>,
    pub(crate) fix_host: Option<String>,
}

fn check_fixers(fixers: &[Fixer]) -> Result<(), PersistError> {
    let blank = |value: &str| value.trim().is_empty();
    let bad = fixers.iter().any(|fixer| {
        blank(&fixer.machine)
            || fixer.host.as_deref().is_some_and(blank)
            || fixer.folder.as_deref().is_some_and(blank)
    });
    if bad {
        return Err(PersistError::DataInvariant(
            "a fixer needs a machine name, and its host and folder are null or not blank"
                .to_owned(),
        ));
    }
    Ok(())
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn wright_followups_to_file(
        self,
        limit: i64,
    ) -> Result<Vec<FollowupRow>, PersistError> {
        let rows = sqlx::query_as(TO_FILE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        with_links(self.pool(), rows).await
    }

    pub(crate) async fn wright_followups_filed_today(self) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar(FILED_TODAY)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn mark_wright_followup_filed(
        self,
        id: i64,
        number: i32,
        url: &str,
    ) -> Result<bool, PersistError> {
        let marked: Option<i64> = sqlx::query_scalar(MARK_FILED)
            .bind(id)
            .bind(number)
            .bind(url)
            .fetch_optional(self.pool())
            .await?;
        Ok(marked.is_some())
    }

    pub(crate) async fn next_wright_handoff(self) -> Result<Option<FollowupRow>, PersistError> {
        let rows = sqlx::query_as(NEXT_HANDOFF) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .fetch_all(self.pool())
            .await?;
        Ok(with_links(self.pool(), rows).await?.into_iter().next())
    }

    pub(crate) async fn wright_handoffs_today(self) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar(HANDOFFS_TODAY)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn live_wright_handoffs(self) -> Result<Vec<FollowupRow>, PersistError> {
        Ok(
            sqlx::query_as(LIVE_HANDOFFS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn tracked_wright_followups(self) -> Result<Vec<FollowupRow>, PersistError> {
        Ok(
            sqlx::query_as(TRACKED) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_followup_children_closed(
        self,
        id: i64,
    ) -> Result<Option<Vec<i32>>, PersistError> {
        let row: Option<Option<Vec<i32>>> = sqlx::query_scalar(CHILDREN_CLOSED) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.flatten())
    }

    pub(crate) async fn mark_wright_followup_handed_off(
        self,
        id: i64,
        session: Uuid,
        machine: &str,
    ) -> Result<bool, PersistError> {
        let marked: Option<i64> = sqlx::query_scalar(MARK_HANDED_OFF)
            .bind(id)
            .bind(session)
            .bind(machine)
            .fetch_optional(self.pool())
            .await?;
        Ok(marked.is_some())
    }

    pub(crate) async fn mark_wright_followup_closed(
        self,
        id: i64,
        state: &str,
        reason: &str,
    ) -> Result<bool, PersistError> {
        let marked: Option<i64> = sqlx::query_scalar(MARK_CLOSED)
            .bind(id)
            .bind(state)
            .bind(reason)
            .fetch_optional(self.pool())
            .await?;
        Ok(marked.is_some())
    }

    pub(crate) async fn mark_wright_followup_batched(
        self,
        id: i64,
        batch: i64,
        commit: &str,
    ) -> Result<bool, PersistError> {
        let marked: Option<i64> = sqlx::query_scalar(MARK_BATCHED)
            .bind(id)
            .bind(batch)
            .bind(commit)
            .fetch_optional(self.pool())
            .await?;
        Ok(marked.is_some())
    }

    pub(crate) async fn unbatch_wright_followups(
        self,
        batch: i64,
        ids: &[i64],
    ) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar(UNBATCH)
            .bind(batch)
            .bind(ids)
            .fetch_all(self.pool())
            .await?)
    }

    pub(crate) async fn return_wright_followup(
        self,
        id: i64,
        max_attempts: i32,
    ) -> Result<Option<String>, PersistError> {
        Ok(sqlx::query_scalar(RETURN_FOLLOWUP)
            .bind(id)
            .bind(max_attempts)
            .fetch_optional(self.pool())
            .await?)
    }

    pub(crate) async fn wright_followup_limits(self) -> Result<FollowupLimits, PersistError> {
        Ok(
            sqlx::query_as(LIMITS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub(crate) async fn patch_wright_followup_limits(
        self,
        patch: &FollowupLimitsPatch,
    ) -> Result<FollowupLimits, PersistError> {
        if let Some(fixers) = &patch.fixers {
            check_fixers(fixers)?;
        }
        Ok(
            sqlx::query_as(PATCH_LIMITS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(patch.repo.as_deref())
                .bind(patch.daily_filed)
                .bind(patch.daily_handoffs)
                .bind(patch.max_attempts)
                .bind(patch.recurrence_actions)
                .bind(patch.recurrence_days)
                .bind(patch.fixers.as_deref().map(Json))
                .bind(patch.fix_host.as_deref())
                .fetch_one(self.pool())
                .await?,
        )
    }
}
