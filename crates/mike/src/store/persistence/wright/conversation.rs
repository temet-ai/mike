use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::FollowupRow;
use crate::store::persistence::PersistError;

const POST: &str = include_str!("sql/conversation/post.sql");
const BY_CLIENT_MSG: &str = include_str!("sql/conversation/by_client_msg.sql");
const MESSAGE_EXISTS: &str = include_str!("sql/conversation/message_exists.sql");
const LIST: &str = include_str!("sql/conversation/list.sql");
const PENDING_REPLY: &str = include_str!("sql/conversation/pending_reply.sql");
const NEXT_DUE: &str = include_str!("sql/conversation/next_due.sql");
const CLAIM_JOB: &str = include_str!("sql/conversation/claim_job.sql");
const RENEW_LEASE: &str = include_str!("sql/conversation/renew_lease.sql");
const RECENT: &str = include_str!("sql/conversation/recent.sql");
const STORE_ANSWER: &str = include_str!("sql/conversation/store_answer.sql");
const FAIL_JOB: &str = include_str!("sql/conversation/fail_job.sql");
const RETRY_JOB: &str = include_str!("sql/conversation/retry_job.sql");
const CLAIM_NOTICES: &str = include_str!("sql/conversation/claim_notices.sql");
const OPERATOR_SESSION: &str = include_str!("sql/conversation/operator_session.sql");
const EXISTING_LINKS: &str = include_str!("sql/conversation/existing_links.sql");
const EVIDENCE: &str = include_str!("sql/conversation/evidence.sql");

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LinkIds {
    pub(crate) problems: Vec<i64>,
    pub(crate) episodes: Vec<i64>,
    pub(crate) actions: Vec<i64>,
    pub(crate) findings: Vec<i64>,
    pub(crate) repairs: Vec<i64>,
    pub(crate) followups: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct MessageRow {
    pub(crate) id: i64,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) client_msg_id: String,
    pub(crate) reply_to: Option<i64>,
    pub(crate) links: Value,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub(crate) struct MessageView {
    pub(crate) id: i64,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) reply_to: Option<i64>,
    pub(crate) links: Value,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) steer_id: Option<i64>,
    pub(crate) steer_change: Option<Value>,
    pub(crate) steer_applied_at: Option<DateTime<Utc>>,
    pub(crate) steer_undone_at: Option<DateTime<Utc>>,
    pub(crate) steer_version: Option<i32>,
    pub(crate) steer_refusal: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct NewOperatorMessage<'a> {
    pub(crate) client_msg_id: &'a str,
    pub(crate) body: &'a str,
    pub(crate) reply_to: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Posted {
    Created(MessageRow),
    Existing(MessageRow),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct ClaimedJob {
    pub(crate) id: i64,
    pub(crate) message_id: i64,
    pub(crate) attempts: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredSteer<'a> {
    pub(crate) change: &'a Value,
    pub(crate) prior: Option<&'a Value>,
    pub(crate) refusal: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredAnswer<'a> {
    pub(crate) job_id: i64,
    pub(crate) body: &'a str,
    pub(crate) links: &'a Value,
    pub(crate) needs_decision: bool,
    pub(crate) steer: Option<StoredSteer<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct AnswerIds {
    pub(crate) answer_id: i64,
    pub(crate) steer_id: Option<i64>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn post_wright_message(
        self,
        message: &NewOperatorMessage<'_>,
    ) -> Result<Posted, PersistError> {
        let inserted: Option<MessageRow> = sqlx::query_as(POST) // grader-allow: runtime query over an include_str! file; wright tables land beside peer branches' migrations (BUILD_PLAN), so no .sqlx entry
            .bind(message.body)
            .bind(message.client_msg_id)
            .bind(message.reply_to)
            .fetch_optional(self.pool())
            .await?;
        if let Some(row) = inserted {
            return Ok(Posted::Created(row));
        }
        let existing: MessageRow = sqlx::query_as(BY_CLIENT_MSG) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
            .bind(message.client_msg_id)
            .fetch_one(self.pool())
            .await?;
        Ok(Posted::Existing(existing))
    }

    pub(crate) async fn wright_message_exists(self, id: i64) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar(MESSAGE_EXISTS)
            .bind(id)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn list_wright_messages(
        self,
        after: i64,
        limit: i64,
    ) -> Result<Vec<MessageView>, PersistError> {
        Ok(
            sqlx::query_as(LIST) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .bind(after)
                .bind(limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_reply_pending(self) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar(PENDING_REPLY)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn wright_reply_next_due(self) -> Result<Option<DateTime<Utc>>, PersistError> {
        Ok(sqlx::query_scalar(NEXT_DUE)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn claim_wright_reply_job(
        self,
        lease_secs: f64,
    ) -> Result<Option<ClaimedJob>, PersistError> {
        Ok(
            sqlx::query_as(CLAIM_JOB) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .bind(lease_secs)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn renew_wright_reply_lease(
        self,
        job_id: i64,
        lease_secs: f64,
    ) -> Result<(), PersistError> {
        sqlx::query(RENEW_LEASE) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
            .bind(job_id)
            .bind(lease_secs)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(crate) async fn recent_wright_messages(
        self,
        up_to: i64,
        limit: i64,
    ) -> Result<Vec<MessageRow>, PersistError> {
        let mut rows: Vec<MessageRow> = sqlx::query_as(RECENT) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
            .bind(up_to)
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        rows.reverse();
        Ok(rows)
    }

    pub(crate) async fn store_wright_answer(
        self,
        answer: &StoredAnswer<'_>,
    ) -> Result<Option<AnswerIds>, PersistError> {
        let steer = answer.steer.as_ref();
        Ok(
            sqlx::query_as(STORE_ANSWER) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .bind(answer.job_id)
                .bind(answer.body)
                .bind(answer.links)
                .bind(answer.needs_decision)
                .bind(steer.map(|s| s.change))
                .bind(steer.and_then(|s| s.prior))
                .bind(steer.and_then(|s| s.refusal))
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn fail_wright_reply_job(
        self,
        job_id: i64,
        reason: &str,
    ) -> Result<bool, PersistError> {
        let failed: Option<i64> = sqlx::query_scalar(FAIL_JOB)
            .bind(job_id)
            .bind(reason)
            .fetch_optional(self.pool())
            .await?;
        Ok(failed.is_some())
    }

    pub(crate) async fn retry_wright_reply_job(
        self,
        job_id: i64,
        delay_secs: f64,
        uncount: bool,
    ) -> Result<(), PersistError> {
        sqlx::query(RETRY_JOB) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
            .bind(job_id)
            .bind(delay_secs)
            .bind(i32::from(uncount))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(crate) async fn claim_wright_notices(self) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar(CLAIM_NOTICES)
            .fetch_all(self.pool())
            .await?)
    }

    pub(crate) async fn wright_operator_session(self) -> Result<Option<Uuid>, PersistError> {
        Ok(sqlx::query_scalar(OPERATOR_SESSION)
            .fetch_optional(self.pool())
            .await?)
    }

    pub(crate) async fn existing_wright_links(
        self,
        ids: &LinkIds,
    ) -> Result<Vec<(String, i64)>, PersistError> {
        Ok(
            sqlx::query_as(EXISTING_LINKS) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .bind(&ids.problems)
                .bind(&ids.episodes)
                .bind(&ids.actions)
                .bind(&ids.findings)
                .bind(&ids.repairs)
                .bind(&ids.followups)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_answer_evidence(
        self,
        named: &[i64],
        limit: i64,
    ) -> Result<Vec<Value>, PersistError> {
        let mut problems: Vec<Value> = sqlx::query_scalar(EVIDENCE)
            .bind(named)
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        for problem in &mut problems {
            let Some(id) = problem.get("problem_id").and_then(Value::as_i64) else {
                continue;
            };
            let followups = self.wright_followups_for_problem(id).await?;
            if let Some(object) = problem.as_object_mut() {
                object.insert(
                    "followups".to_owned(),
                    Value::Array(followups.iter().map(followup_evidence).collect()),
                );
            }
        }
        Ok(problems)
    }
}

fn followup_evidence(row: &FollowupRow) -> Value {
    serde_json::json!({
        "followup_id": row.id,
        "kind": row.kind,
        "state": row.state,
        "title": row.title,
        "issue_number": row.issue_number,
        "issue_url": row.issue_url,
        "fixer_session": row.fixer_session,
        "fixer_machine": row.fixer_machine,
    })
}
