use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Default)]
pub(crate) struct CompositionUsageStats {
    pub(crate) count_1h: i64,
    pub(crate) count_24h: i64,
    pub(crate) count_7d: i64,
    pub(crate) count_total: i64,
    pub(crate) total_input_tokens: i64,
    pub(crate) total_output_tokens: i64,
    pub(crate) recent: Vec<CompositionUsageSample>,
}

#[derive(Debug, Clone, sqlx::FromRow, Deserialize)]
pub(crate) struct CompositionUsageSample {
    pub(crate) request_id: i64,
    pub(crate) session_id: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) model: Option<String>,
    pub(crate) input_tokens: Option<i64>,
    pub(crate) output_tokens: Option<i64>,
    pub(crate) cache_read_tokens: Option<i64>,
    pub(crate) cache_write_tokens: Option<i64>,
}

impl PersistHandle {
    pub(crate) async fn count_applied_24h(&self, composition_id: i64) -> Result<i64, PersistError> {
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

    pub(crate) async fn composition_usage_stats(
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

    pub(crate) async fn count_applied_24h_all(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::compositions::NewCompositionParams;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    use crate::store::models::{InterceptedRequest, RequestStatus};

    async fn snapshot_via_handle(
        handle: &PersistHandle,
        session_id: &str,
        memory_id: u64,
        body_marker: &str,
        configure: impl FnOnce(&mut InterceptedRequest),
    ) -> anyhow::Result<()> {
        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(memory_id)),
            session_id,
        );
        req.status = RequestStatus::Complete;
        req.request_body = Some(bytes::Bytes::from(format!("req-body-{body_marker}")));
        req.response_body = Some(bytes::Bytes::from(format!("resp-body-{body_marker}")));
        req.completed_at = Some(req.created_at);
        configure(&mut req);
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;
        handle.snapshot_request(&req).await?;
        Ok(())
    }

    #[tokio::test]
    async fn composition_usage_stats_returns_totals_and_recent_in_one_round_trip()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let sha = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let blocks = serde_json::json!([{ "type": "text", "text": "hello" }]);
        let composition = handle
            .create_composition(NewCompositionParams {
                prompt_sha256: &sha,
                label: "usage-stats-test",
                blocks: &blocks,
                mode: None,
                agent_name: None,
                agent_definition_path: None,
            })
            .await?;

        let base = chrono::Utc::now();
        for i in 0..5u64 {
            let created = base + chrono::Duration::milliseconds(i as i64 * 10);
            let sha = sha.clone();
            let blocks = blocks.clone();
            snapshot_via_handle(&handle, &session_id, 50_000 + i, "u", move |req| {
                req.created_at = created;
                req.completed_at = Some(created);
                req.system_prompt.prompt_sha256 = Some(sha);
                req.system_prompt.blocks = Some(blocks);
                req.token_usage = Some(crate::types::TokenUsage {
                    input_tokens: 100 + (i as u32) * 10,
                    output_tokens: 200 + (i as u32) * 10,
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                });
            })
            .await?;
        }

        let stats = handle.composition_usage_stats(composition.id, 3).await?;
        assert_eq!(stats.count_total, 5, "all 5 seeded rows must be counted");
        assert_eq!(stats.count_1h, 5);
        assert_eq!(stats.count_24h, 5);
        assert_eq!(stats.count_7d, 5);
        assert_eq!(stats.total_input_tokens, 600);
        assert_eq!(stats.total_output_tokens, 1100);
        assert_eq!(
            stats.recent.len(),
            3,
            "limit=3 must cap the recent list regardless of window count"
        );
        assert_eq!(stats.recent[0].input_tokens, Some(140));
        assert_eq!(stats.recent[1].input_tokens, Some(130));
        assert_eq!(stats.recent[2].input_tokens, Some(120));

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn composition_usage_stats_returns_zero_and_empty_for_unknown_composition()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let stats = handle.composition_usage_stats(i64::MAX, 50).await?;
        assert_eq!(stats.count_total, 0);
        assert_eq!(stats.count_1h, 0);
        assert_eq!(stats.count_24h, 0);
        assert_eq!(stats.count_7d, 0);
        assert_eq!(stats.total_input_tokens, 0);
        assert_eq!(stats.total_output_tokens, 0);
        assert!(stats.recent.is_empty(), "empty recent list expected");

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn count_applied_24h_counts_only_override_applied_requests_per_composition()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let sha = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let blocks = serde_json::json!([{ "type": "text", "text": "hello" }]);
        let composition = handle
            .create_composition(NewCompositionParams {
                prompt_sha256: &sha,
                label: "applied-count-test",
                blocks: &blocks,
                mode: None,
                agent_name: None,
                agent_definition_path: None,
            })
            .await?;

        for (i, applied) in [true, true, false].into_iter().enumerate() {
            let sha = sha.clone();
            let blocks = blocks.clone();
            let composition_id = composition.id;
            snapshot_via_handle(&handle, &session_id, 60_000 + i as u64, "a", move |req| {
                req.system_prompt.prompt_sha256 = Some(sha);
                req.system_prompt.blocks = Some(blocks);
                req.system_prompt.composition_id = Some(composition_id);
                req.system_prompt.composition_applied = applied;
            })
            .await?;
        }

        assert_eq!(handle.count_applied_24h(composition.id).await?, 2);
        let all = handle.count_applied_24h_all().await?;
        assert_eq!(all.get(&composition.id).copied(), Some(2));

        db.cleanup().await?;
        Ok(())
    }
}
