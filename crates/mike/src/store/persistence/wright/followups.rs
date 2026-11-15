use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::store::persistence::PersistError;

const RECORD: &str = include_str!("sql/followups/record.sql");
const LIVE: &str = include_str!("sql/followups/live.sql");
const LIST: &str = include_str!("sql/followups/list.sql");
const FOR_PROBLEM: &str = include_str!("sql/followups/for_problem.sql");
const LINKS: &str = include_str!("sql/followups/links.sql");
const RECURRING: &str = include_str!("sql/followups/recurring.sql");
const OPEN_ROOT_CAUSE: &str = include_str!("sql/followups/open_root_cause.sql");
const PICK_ROOT_CAUSE: &str = include_str!("sql/followups/pick_root_cause.sql");

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct FollowupRow {
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) check_name: Option<String>,
    pub(crate) owning_part: Option<String>,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) state: String,
    pub(crate) issue_number: Option<i32>,
    pub(crate) issue_url: Option<String>,
    pub(crate) attempts: i32,
    pub(crate) fixer_session: Option<Uuid>,
    pub(crate) fixer_machine: Option<String>,
    pub(crate) filed_at: Option<DateTime<Utc>>,
    pub(crate) handed_off_at: Option<DateTime<Utc>>,
    pub(crate) closed_at: Option<DateTime<Utc>>,
    pub(crate) closed_reason: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    #[sqlx(default)]
    pub(crate) findings: Vec<i64>,
    #[sqlx(default)]
    pub(crate) problems: Vec<i64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NewFollowup<'a> {
    pub(crate) kind: &'a str,
    pub(crate) check_name: Option<&'a str>,
    pub(crate) owning_part: Option<&'a str>,
    pub(crate) title: &'a str,
    pub(crate) body: &'a str,
    pub(crate) finding_id: i64,
    pub(crate) same_as: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct Recurring {
    pub(crate) check_name: String,
    pub(crate) remedy: String,
    pub(crate) actions: i64,
    pub(crate) newest_problem_id: i64,
    pub(crate) newest_episode_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct RootCausePick {
    pub(crate) followup_id: i64,
    pub(crate) problem_id: i64,
    pub(crate) episode_id: i64,
    pub(crate) check_name: String,
    pub(crate) title: String,
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    followup_id: i64,
    findings: Vec<i64>,
    problems: Vec<i64>,
}

pub(super) async fn with_links(
    pool: &PgPool,
    mut rows: Vec<FollowupRow>,
) -> Result<Vec<FollowupRow>, PersistError> {
    if rows.is_empty() {
        return Ok(rows);
    }
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let links: Vec<LinkRow> = sqlx::query_as(LINKS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
        .bind(&ids)
        .fetch_all(pool)
        .await?;
    let mut by_id: HashMap<i64, LinkRow> = links
        .into_iter()
        .map(|link| (link.followup_id, link))
        .collect();
    for row in &mut rows {
        if let Some(link) = by_id.remove(&row.id) {
            row.findings = link.findings;
            row.problems = link.problems;
        }
    }
    Ok(rows)
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn record_wright_followup(
        self,
        followup: &NewFollowup<'_>,
    ) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar(RECORD)
            .bind(followup.kind)
            .bind(followup.check_name)
            .bind(followup.owning_part)
            .bind(followup.title)
            .bind(followup.body)
            .bind(followup.finding_id)
            .bind(followup.same_as)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn live_wright_followups(
        self,
        check_name: Option<&str>,
        owning_part: Option<&str>,
    ) -> Result<Vec<FollowupRow>, PersistError> {
        Ok(
            sqlx::query_as(LIVE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(check_name)
                .bind(owning_part)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn list_wright_followups(
        self,
        state: Option<&str>,
        limit: i64,
        before_id: Option<i64>,
    ) -> Result<Vec<FollowupRow>, PersistError> {
        let rows = sqlx::query_as(LIST) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(state)
            .bind(limit)
            .bind(before_id)
            .fetch_all(self.pool())
            .await?;
        with_links(self.pool(), rows).await
    }

    pub(crate) async fn wright_followups_for_problem(
        self,
        problem_id: i64,
    ) -> Result<Vec<FollowupRow>, PersistError> {
        let rows = sqlx::query_as(FOR_PROBLEM) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(problem_id)
            .fetch_all(self.pool())
            .await?;
        with_links(self.pool(), rows).await
    }

    pub(crate) async fn recurring_wright_remedies(
        self,
        days: i32,
        min_actions: i64,
    ) -> Result<Vec<Recurring>, PersistError> {
        Ok(
            sqlx::query_as(RECURRING) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(days)
                .bind(min_actions)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn open_root_cause_followup(
        self,
        check_name: &str,
        remedy: &str,
        actions: i64,
        days: i32,
    ) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar(OPEN_ROOT_CAUSE)
            .bind(check_name)
            .bind(remedy)
            .bind(actions)
            .bind(days)
            .fetch_optional(self.pool())
            .await?)
    }

    pub(crate) async fn pick_wright_root_cause(
        self,
        max_attempts: i64,
    ) -> Result<Option<RootCausePick>, PersistError> {
        Ok(
            sqlx::query_as(PICK_ROOT_CAUSE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(max_attempts)
                .fetch_optional(self.pool())
                .await?,
        )
    }
}
