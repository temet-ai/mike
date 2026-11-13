use chrono::{DateTime, Utc};

use super::super::PersistError;

pub const GOLD_BY_AGENT_PATTERN: &str =
    r#"(curl|wget|xh|http)\s[^\n]*(localhost|127\.0\.0\.1|\[::1\])[^\s'"]*/api/eval/labels"#;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SubjectDetail {
    pub subject: String,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct RefusalRow {
    pub subject: String,
    pub detail: serde_json::Value,
    pub fires: bool,
}

pub fn budget_setting_keys() -> Vec<String> {
    let mut keys: Vec<String> = [
        "llm.ceiling_max_calls",
        "llm.ceiling_window_secs",
        "llm.rate_limit_per_minute",
        "llm.daily_spend_ceiling_usd",
    ]
    .iter()
    .map(|k| (*k).to_owned())
    .collect();
    for role in [
        "steerer",
        "session_judge",
        "judge_worker",
        "lesson_relations",
        "eval",
    ] {
        keys.push(format!("llm.budget.{role}.share_pct"));
    }
    keys
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn eval_refusals_at(
        self,
        at: DateTime<Utc>,
        min_refused: i64,
        min_share: f64,
    ) -> Result<Vec<RefusalRow>, PersistError> {
        let rows = self
            .eval_refusals_many(&[at], min_refused, min_share)
            .await?;
        Ok(rows.into_iter().map(|(_, row)| row).collect())
    }

    pub async fn eval_refusals_many(
        self,
        ats: &[DateTime<Utc>],
        min_refused: i64,
        min_share: f64,
    ) -> Result<Vec<(DateTime<Utc>, RefusalRow)>, PersistError> {
        let setting_keys = budget_setting_keys();
        crate::metrics::time_db("eval_alarm_budget_refusals", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/eval_metrics/refusals.sql",
                min_refused,
                min_share,
                &setting_keys,
                ats,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| {
                    (
                        r.at,
                        RefusalRow {
                            subject: r.subject,
                            detail: r.detail,
                            fires: r.fires,
                        },
                    )
                })
                .collect())
        })
        .await
    }

    pub async fn eval_ungated_changes(
        self,
        roles: &[String],
        grace_hours: i64,
    ) -> Result<Vec<SubjectDetail>, PersistError> {
        let grace = i32::try_from(grace_hours).unwrap_or(i32::MAX);
        crate::metrics::time_db("eval_alarm_change_without_gate", async move {
            let mut rows = sqlx::query_file_as!(
                SubjectDetail,
                "src/store/persistence/eval_metrics/gate.sql",
                roles,
                grace,
            )
            .fetch_all(&self.pool)
            .await?;
            let unstamped = sqlx::query_file_as!(
                SubjectDetail,
                "src/store/persistence/eval_metrics/unstamped.sql",
                roles,
            )
            .fetch_all(&self.pool)
            .await?;
            rows.extend(unstamped);
            Ok(rows)
        })
        .await
    }

    pub async fn eval_gold_by_agent_at(
        self,
        at: DateTime<Utc>,
    ) -> Result<Vec<SubjectDetail>, PersistError> {
        let rows = self.eval_gold_by_agent_many(&[at]).await?;
        Ok(rows.into_iter().map(|(_, row)| row).collect())
    }

    pub async fn eval_gold_by_agent_many(
        self,
        ats: &[DateTime<Utc>],
    ) -> Result<Vec<(DateTime<Utc>, SubjectDetail)>, PersistError> {
        crate::metrics::time_db("eval_alarm_gold_label_by_agent", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/eval_metrics/gold_by_agent.sql",
                ats,
                GOLD_BY_AGENT_PATTERN,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| {
                    (
                        r.at,
                        SubjectDetail {
                            subject: r.subject,
                            detail: r.detail,
                        },
                    )
                })
                .collect())
        })
        .await
    }

    pub async fn eval_last_judge_verdict_at(self) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("eval_alarm_judge_verdict_count", async move {
            Ok(sqlx::query_scalar!(
                "SELECT max(classified_at) FROM memory.operator_turns
                  WHERE judge_model IS NOT NULL AND created_at >= now() - interval '30 days'",
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn eval_judge_call_outcomes(
        self,
        hours: i64,
    ) -> Result<serde_json::Value, PersistError> {
        let hours = i32::try_from(hours).unwrap_or(i32::MAX);
        crate::metrics::time_db("eval_judge_call_outcomes", async move {
            Ok(sqlx::query_scalar!(
                r#"WITH c AS (
                       SELECT outcome, count(*) AS n FROM llm.calls
                        WHERE role = 'judge_worker' AND created_at >= now() - make_interval(hours => $1)
                        GROUP BY outcome
                   )
                   SELECT coalesce(jsonb_object_agg(outcome, n), '{}'::jsonb) AS "outcomes!" FROM c"#,
                hours,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
