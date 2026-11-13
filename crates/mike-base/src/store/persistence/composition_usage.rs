use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Default)]
pub struct CompositionUsageStats {
    pub count_1h: i64,
    pub count_24h: i64,
    pub count_7d: i64,
    pub count_total: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub recent: Vec<CompositionUsageSample>,
}

#[derive(Debug, Clone, sqlx::FromRow, Deserialize)]
pub struct CompositionUsageSample {
    pub request_id: i64,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
}

impl PersistHandle {
    pub async fn count_applied_24h(&self, composition_id: i64) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_applied_24h", async move {
            let applied = sqlx::query_scalar!(
                r#"SELECT COUNT(*) AS "applied!"
                     FROM overrides.overrides o
                     JOIN requests.system_prompts rsp ON rsp.system_prompt_id = o.system_prompt_id
                     JOIN requests.requests r ON r.id = rsp.request_id
                    WHERE o.id = $1
                      AND rsp.override_applied
                      AND r.created_at > NOW() - INTERVAL '24 hours'"#,
                composition_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(applied)
        })
        .await
    }

    pub async fn composition_usage_stats(
        &self,
        composition_id: i64,
        limit: i64,
    ) -> Result<CompositionUsageStats, PersistError> {
        crate::metrics::time_db("composition_usage_stats", async move {
            let row = sqlx::query!(
                r#"WITH matched AS (
                       SELECT r.id,
                              s.session_uuid::text AS session_id,
                              r.created_at,
                              r.model,
                              rr.token_usage
                         FROM overrides.overrides o
                         JOIN requests.system_prompts rsp ON rsp.system_prompt_id = o.system_prompt_id
                         JOIN requests.requests r ON r.id = rsp.request_id
                         JOIN sessions.sessions s ON s.id = r.session_id
                         LEFT JOIN responses.responses rr ON rr.request_id = r.id
                        WHERE o.id = $1
                   ),
                   totals AS (
                       SELECT COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '1 hour') AS count_1h,
                              COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '24 hours') AS count_24h,
                              COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '7 days') AS count_7d,
                              COUNT(*) AS count_total,
                              SUM((token_usage->>'input_tokens')::bigint)::bigint AS total_input_tokens,
                              SUM((token_usage->>'output_tokens')::bigint)::bigint AS total_output_tokens
                         FROM matched
                   ),
                   recent AS (
                       SELECT COALESCE(jsonb_agg(to_jsonb(top) ORDER BY top.created_at DESC), '[]'::jsonb) AS samples
                         FROM (
                              SELECT id AS request_id,
                                     session_id,
                                     created_at,
                                     model,
                                     (token_usage->>'input_tokens')::bigint AS input_tokens,
                                     (token_usage->>'output_tokens')::bigint AS output_tokens,
                                     (token_usage->>'cache_read_input_tokens')::bigint AS cache_read_tokens,
                                     (token_usage->>'cache_creation_input_tokens')::bigint AS cache_write_tokens
                                FROM matched
                               ORDER BY created_at DESC
                               LIMIT $2
                         ) top
                   )
                   SELECT t.count_1h AS "count_1h!",
                          t.count_24h AS "count_24h!",
                          t.count_7d AS "count_7d!",
                          t.count_total AS "count_total!",
                          t.total_input_tokens,
                          t.total_output_tokens,
                          recent.samples AS "samples!: sqlx::types::Json<Vec<CompositionUsageSample>>"
                     FROM totals t
                    CROSS JOIN recent"#,
                composition_id,
                limit,
            )
            .fetch_one(&self.pool)
            .await?;

            Ok(CompositionUsageStats {
                count_1h: row.count_1h,
                count_24h: row.count_24h,
                count_7d: row.count_7d,
                count_total: row.count_total,
                total_input_tokens: row.total_input_tokens.unwrap_or(0),
                total_output_tokens: row.total_output_tokens.unwrap_or(0),
                recent: row.samples.0,
            })
        })
        .await
    }

    pub async fn count_applied_24h_all(
        &self,
    ) -> Result<std::collections::HashMap<i64, i64>, PersistError> {
        crate::metrics::time_db("count_applied_24h_all", async move {
            let rows = sqlx::query!(
                r#"SELECT o.id AS "composition_id!", COUNT(*) AS "applied!"
                     FROM overrides.overrides o
                     JOIN requests.system_prompts rsp ON rsp.system_prompt_id = o.system_prompt_id
                     JOIN requests.requests r ON r.id = rsp.request_id
                    WHERE rsp.override_applied
                      AND r.created_at > NOW() - INTERVAL '24 hours'
                    GROUP BY o.id"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| (r.composition_id, r.applied))
                .collect())
        })
        .await
    }
}
