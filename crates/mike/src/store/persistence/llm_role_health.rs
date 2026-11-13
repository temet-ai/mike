use chrono::{DateTime, Utc};

use super::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRoleOutcome {
    pub role: String,
    pub model: String,
    pub last_outcome: String,
    pub last_http_status: Option<i32>,
    pub last_call_at: DateTime<Utc>,
    pub last_ok_at: Option<DateTime<Utc>>,
    pub failed_streak: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerToggleState {
    pub key: String,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub async fn llm_role_outcomes(self) -> Result<Vec<LlmRoleOutcome>, PersistError> {
        crate::metrics::time_db("llm_role_outcomes", async move {
            Ok(sqlx::query_as!(
                LlmRoleOutcome,
                r#"WITH recent AS (
                     SELECT role, model, outcome, http_status, created_at,
                            row_number() OVER (PARTITION BY role ORDER BY created_at DESC, id DESC) AS rn
                       FROM llm.calls
                      WHERE created_at > now() - interval '7 days'
                        AND role <> 'eval'
                   )
                   SELECT role AS "role!",
                          (array_agg(model ORDER BY rn))[1] AS "model!",
                          (array_agg(outcome ORDER BY rn))[1] AS "last_outcome!",
                          (array_agg(http_status ORDER BY rn))[1] AS last_http_status,
                          max(created_at) AS "last_call_at!",
                          max(created_at) FILTER (WHERE outcome = 'ok') AS last_ok_at,
                          COALESCE(min(rn) FILTER (WHERE outcome = 'ok') - 1, count(*))::bigint AS "failed_streak!"
                     FROM recent
                    GROUP BY role
                    ORDER BY role"#
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn worker_toggle_states(
        self,
        keys: &[String],
    ) -> Result<Vec<WorkerToggleState>, PersistError> {
        crate::metrics::time_db("worker_toggle_states", async move {
            Ok(sqlx::query_as!(
                WorkerToggleState,
                "SELECT key, enabled, updated_at
                   FROM config.feature_toggles
                  WHERE key = ANY($1)
                  ORDER BY key",
                keys
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn corrected_turns_since(self, since: DateTime<Utc>) -> Result<i64, PersistError> {
        crate::metrics::time_db("corrected_turns_since", async move {
            let outcomes = sqlx::query!(
                r#"SELECT id::bigint AS "id!", code AS "code!"
                     FROM memory.turn_outcomes
                    WHERE code IN ('corrected', 'abandoned')"#
            )
            .fetch_all(&self.pool)
            .await?;
            let ids: Vec<i64> = outcomes.iter().map(|o| o.id).collect();
            let Some(corrected) = outcomes
                .iter()
                .find_map(|o| (o.code == "corrected").then_some(o.id))
            else {
                return Ok(0);
            };
            Ok(sqlx::query_scalar!(
                r#"SELECT count(*)::bigint AS "count!"
                     FROM memory.operator_turns t
                    WHERE t.created_at >= $1
                      AND t.outcome_id = ANY($2)
                      AND (t.outcome_id = $3
                           OR t.outcome_source IS NOT DISTINCT FROM 'frustration_close')"#,
                since,
                &ids,
                corrected,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
