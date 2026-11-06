use chrono::{DateTime, NaiveDate, Utc};

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone)]
pub struct LlmCallInsert<'a> {
    pub role: &'a str,
    pub model: &'a str,
    pub generation_id: Option<&'a str>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub reasoning_tokens: Option<i32>,
    pub cached_tokens: Option<i32>,
    pub cost_usd: Option<f64>,
    pub latency_ms: Option<i32>,
    pub outcome: &'a str,
    pub http_status: Option<i32>,
    pub session_id: Option<i64>,
    pub steerer_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub request_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct LlmSpendGroupRow {
    pub key: String,
    pub call_count: i64,
    pub cost_usd: f64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub subscription_call_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LlmCallStatus {
    pub http_status: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct LlmSpendCallerRow {
    pub role: String,
    pub steerer_id: Option<i64>,
    pub steerer_name: Option<String>,
    pub steerer_display_name: Option<String>,
    pub call_count: i64,
    pub cost_usd: f64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub failed_count: i64,
    pub subscription_call_count: i64,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct LlmSpendDayCallerRow {
    pub day: String,
    pub role: String,
    pub steerer_id: Option<i64>,
    pub steerer_name: Option<String>,
    pub steerer_display_name: Option<String>,
    pub cost_usd: f64,
    pub call_count: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub failed_count: i64,
    pub subscription_call_count: i64,
}

pub fn fold_callers(rows: &[LlmSpendDayCallerRow]) -> Vec<LlmSpendCallerRow> {
    let mut by_caller: std::collections::BTreeMap<(String, Option<i64>), LlmSpendCallerRow> =
        std::collections::BTreeMap::new();
    for row in rows {
        let entry = by_caller
            .entry((row.role.clone(), row.steerer_id))
            .or_insert_with(|| LlmSpendCallerRow {
                role: row.role.clone(),
                steerer_id: row.steerer_id,
                steerer_name: row.steerer_name.clone(),
                steerer_display_name: row.steerer_display_name.clone(),
                call_count: 0,
                cost_usd: 0.0,
                prompt_tokens: 0,
                completion_tokens: 0,
                reasoning_tokens: 0,
                failed_count: 0,
                subscription_call_count: 0,
            });
        entry.call_count += row.call_count;
        entry.cost_usd += row.cost_usd;
        entry.prompt_tokens += row.prompt_tokens;
        entry.completion_tokens += row.completion_tokens;
        entry.reasoning_tokens += row.reasoning_tokens;
        entry.failed_count += row.failed_count;
        entry.subscription_call_count += row.subscription_call_count;
    }
    by_caller.into_values().collect()
}

const FIRST_DAY: NaiveDate = match NaiveDate::from_ymd_opt(1, 1, 1) {
    Some(day) => day,
    None => NaiveDate::MIN,
};

const LAST_DAY: NaiveDate = match NaiveDate::from_ymd_opt(9999, 12, 31) {
    Some(day) => day,
    None => NaiveDate::MAX,
};

fn day_window(
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> (NaiveDate, NaiveDate) {
    (
        since.map_or(FIRST_DAY, |t| t.date_naive()),
        until.map_or(LAST_DAY, |t| t.date_naive()),
    )
}

#[derive(Clone, Copy)]
enum SpendGroup {
    Day,
    Model,
    Role,
}

impl PersistHandle {
    pub async fn recent_llm_call_statuses(
        &self,
        limit: i64,
    ) -> Result<Vec<LlmCallStatus>, PersistError> {
        crate::metrics::time_db("recent_llm_call_statuses", async move {
            Ok(sqlx::query_file_as!(
                LlmCallStatus,
                "src/store/persistence/llm_calls/recent_statuses.sql",
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn insert_llm_call(&self, call: LlmCallInsert<'_>) -> Result<i64, PersistError> {
        let machine_uuid = self.llm_machine_uuid().await;
        crate::metrics::time_db("insert_llm_call", async move {
            let id = sqlx::query_scalar!(
                r#"INSERT INTO llm.calls
                       (role, model, generation_id, prompt_tokens, completion_tokens,
                        reasoning_tokens, cached_tokens, cost_usd, latency_ms, outcome,
                        http_status, session_id, steerer_id, operator_turn_id, request_body,
                        machine_uuid)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
                   RETURNING id"#,
                call.role,
                call.model,
                call.generation_id,
                call.prompt_tokens,
                call.completion_tokens,
                call.reasoning_tokens,
                call.cached_tokens,
                call.cost_usd,
                call.latency_ms,
                call.outcome,
                call.http_status,
                call.session_id,
                call.steerer_id,
                call.operator_turn_id,
                call.request_body as Option<serde_json::Value>,
                machine_uuid,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn sum_todays_llm_spend_usd(&self) -> Result<f64, PersistError> {
        crate::metrics::time_db("sum_todays_llm_spend_usd", async move {
            let total = sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(cost_usd), 0.0)::float8 AS "total!"
                     FROM llm.daily_spend
                    WHERE day = (now() AT TIME ZONE 'utc')::date"#
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(total)
        })
        .await
    }

    pub async fn sum_llm_spend_range(
        &self,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<f64, PersistError> {
        crate::metrics::time_db("sum_llm_spend_range", async move {
            let (since_day, until_day) = day_window(Some(since), until);
            let total = sqlx::query_file_scalar!(
                "src/store/persistence/llm_calls/sum_spend_range.sql",
                since_day,
                until_day,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(total)
        })
        .await
    }

    pub async fn llm_spend_by_day(
        &self,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<LlmSpendGroupRow>, PersistError> {
        crate::metrics::time_db(
            "llm_spend_by_day",
            self.llm_spend_grouped(SpendGroup::Day, since, until),
        )
        .await
    }

    pub async fn llm_spend_by_model(
        &self,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<LlmSpendGroupRow>, PersistError> {
        crate::metrics::time_db(
            "llm_spend_by_model",
            self.llm_spend_grouped(SpendGroup::Model, since, until),
        )
        .await
    }

    pub async fn llm_spend_by_role(
        &self,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<LlmSpendGroupRow>, PersistError> {
        crate::metrics::time_db(
            "llm_spend_by_role",
            self.llm_spend_grouped(SpendGroup::Role, since, until),
        )
        .await
    }

    async fn llm_spend_grouped(
        &self,
        group: SpendGroup,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<LlmSpendGroupRow>, PersistError> {
        let (since_day, until_day) = day_window(since, until);
        let rows = match group {
            SpendGroup::Day => {
                sqlx::query_file_as!(
                    LlmSpendGroupRow,
                    "src/store/persistence/llm_calls/spend_by_day.sql",
                    since_day,
                    until_day,
                )
                .fetch_all(&self.pool)
                .await?
            }
            SpendGroup::Model => {
                sqlx::query_file_as!(
                    LlmSpendGroupRow,
                    "src/store/persistence/llm_calls/spend_by_model.sql",
                    since_day,
                    until_day,
                )
                .fetch_all(&self.pool)
                .await?
            }
            SpendGroup::Role => {
                sqlx::query_file_as!(
                    LlmSpendGroupRow,
                    "src/store/persistence/llm_calls/spend_by_role.sql",
                    since_day,
                    until_day,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows)
    }

    pub async fn llm_spend_by_day_caller(
        &self,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<LlmSpendDayCallerRow>, PersistError> {
        crate::metrics::time_db("llm_spend_by_day_caller", async move {
            let (since_day, until_day) = day_window(since, until);
            let rows = sqlx::query_file_as!(
                LlmSpendDayCallerRow,
                "src/store/persistence/llm_calls/spend_by_day_caller.sql",
                since_day,
                until_day,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}

#[cfg(test)]
mod fold_callers_tests {
    use super::{LlmSpendDayCallerRow, fold_callers};

    fn day(
        day: &str,
        role: &str,
        steerer: Option<i64>,
        cost: f64,
        calls: i64,
    ) -> LlmSpendDayCallerRow {
        LlmSpendDayCallerRow {
            day: day.to_owned(),
            role: role.to_owned(),
            steerer_id: steerer,
            steerer_name: steerer.map(|_| "verifier".to_owned()),
            steerer_display_name: steerer.map(|_| "Verifier".to_owned()),
            cost_usd: cost,
            call_count: calls,
            prompt_tokens: calls * 100,
            completion_tokens: calls,
            reasoning_tokens: 0,
            failed_count: 1,
            subscription_call_count: calls / 2,
        }
    }

    #[test]
    fn subscription_filter_matches_the_codex_and_claude_model_prefixes() {
        for sql in [
            include_str!("llm_calls/spend_by_day.sql"),
            include_str!("llm_calls/spend_by_model.sql"),
            include_str!("llm_calls/spend_by_role.sql"),
            include_str!("llm_calls/spend_by_day_caller.sql"),
        ] {
            assert!(sql.contains(crate::llm::codex::MODEL_PREFIX));
            assert!(sql.contains(crate::llm::claude::MODEL_PREFIX));
        }
    }

    #[test]
    fn fold_callers_sums_subscription_calls() {
        let rows = [
            day("2026-09-23", "judge_worker", None, 0.0, 40),
            day("2026-09-24", "judge_worker", None, 0.0, 60),
        ];
        let folded = fold_callers(&rows);
        assert_eq!(folded.len(), 1);
        assert!(
            folded
                .first()
                .is_some_and(|r| r.subscription_call_count == 50)
        );
    }

    #[test]
    fn fold_callers_sums_each_caller_across_days() {
        let rows = [
            day("2026-09-23", "steerer", Some(6), 2.0, 40),
            day("2026-09-24", "steerer", Some(6), 3.0, 60),
            day("2026-09-24", "judge_worker", None, 1.0, 500),
        ];
        let folded = fold_callers(&rows);
        assert_eq!(folded.len(), 2);
        let verifier = folded.iter().find(|r| r.steerer_id == Some(6));
        assert!(verifier.is_some_and(|v| (v.cost_usd - 5.0).abs() < 1e-9
            && v.call_count == 100
            && v.prompt_tokens == 10_000
            && v.failed_count == 2
            && v.steerer_display_name.as_deref() == Some("Verifier")));
    }

    #[test]
    fn fold_callers_of_nothing_is_empty() {
        assert!(fold_callers(&[]).is_empty());
    }
}
