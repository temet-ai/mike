use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};

use super::super::super::PersistError;
use super::types::{
    PromptClassDetailRow, PromptClassObservationRow, PromptClassRow, PromptClassScope,
    PromptClassVersionRow,
};

const LIST_TAIL: &str = include_str!("prompt_classes/list_tail.sql");
const DETAIL_TAIL: &str = include_str!("prompt_classes/detail_tail.sql");

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn list_prompt_classes(
        self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
        project_path: Option<&str>,
    ) -> Result<Vec<PromptClassRow>, PersistError> {
        crate::metrics::time_db("list_prompt_classes", async {
            let mut query = QueryBuilder::<Postgres>::new(
                "WITH windowed AS MATERIALIZED (
                    SELECT sp.lane, sp.subagent_persona, sp.blocks, sp.created_at, r.model
                    FROM system_prompts.session_prompts sp
                    JOIN requests.requests r ON r.id = sp.request_id",
            );
            if let Some(path) = project_path {
                query
                    .push(" JOIN projects.projects p ON p.id = r.project_id AND p.project_path = ")
                    .push_bind(path);
            }
            query
                .push(" WHERE sp.created_at >= COALESCE(")
                .push_bind(window_start)
                .push("::timestamptz, NOW() - INTERVAL '7 days') AND sp.created_at < COALESCE(")
                .push_bind(window_end)
                .push("::timestamptz, NOW()) ")
                .push(LIST_TAIL)
                .push(" LIMIT ")
                .push_bind(limit)
                .push(" OFFSET ")
                .push_bind(offset);
            Ok(query
                .build_query_as::<PromptClassRow>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }

    pub async fn get_prompt_class_detail(
        self,
        lane: &str,
        persona: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        scope: PromptClassScope<'_>,
    ) -> Result<Option<PromptClassDetailRow>, PersistError> {
        crate::metrics::time_db("get_prompt_class_detail", async {
            let mut query = QueryBuilder::<Postgres>::new(
                "WITH windowed AS MATERIALIZED (
                    SELECT sp.blocks, sp.created_at, r.model
                    FROM system_prompts.session_prompts sp
                    JOIN requests.requests r ON r.id = sp.request_id",
            );
            if let Some(path) = scope.project_path {
                query
                    .push(" JOIN projects.projects p ON p.id = r.project_id AND p.project_path = ")
                    .push_bind(path);
            }
            query
                .push(" WHERE sp.lane = ")
                .push_bind(lane)
                .push(" AND sp.subagent_persona IS NOT DISTINCT FROM ")
                .push_bind(persona)
                .push(" AND sp.created_at >= COALESCE(")
                .push_bind(window_start)
                .push("::timestamptz, NOW() - INTERVAL '7 days') AND sp.created_at < COALESCE(")
                .push_bind(window_end)
                .push("::timestamptz, NOW())");
            if let Some(model) = scope.model {
                query.push(" AND r.model = ").push_bind(model);
            }
            query.push(" ").push(DETAIL_TAIL);
            Ok(query
                .build_query_as::<PromptClassDetailRow>()
                .fetch_optional(&self.pool)
                .await?)
        })
        .await
    }

    pub async fn list_prompt_class_observations(
        self,
        lane: &str,
        persona: Option<&str>,
        limit: i64,
        offset: i64,
        scope: PromptClassScope<'_>,
    ) -> Result<Vec<PromptClassObservationRow>, PersistError> {
        crate::metrics::time_db("list_prompt_class_observations", async {
            let mut query = QueryBuilder::<Postgres>::new(
                "SELECT sp.request_id, sp.session_id, sp.created_at, sp.content_sha256,
                        sp.altered, r.model,
                        COUNT(*) OVER ()::bigint AS total_count
                 FROM system_prompts.session_prompts sp
                 LEFT JOIN requests.requests r ON r.id = sp.request_id",
            );
            if let Some(path) = scope.project_path {
                query
                    .push(" JOIN projects.projects p ON p.id = r.project_id AND p.project_path = ")
                    .push_bind(path);
            }
            query
                .push(" WHERE sp.lane = ")
                .push_bind(lane)
                .push(" AND sp.subagent_persona IS NOT DISTINCT FROM ")
                .push_bind(persona);
            if let Some(model) = scope.model {
                query.push(" AND r.model = ").push_bind(model);
            }
            query
                .push(" ORDER BY sp.created_at DESC LIMIT ")
                .push_bind(limit)
                .push(" OFFSET ")
                .push_bind(offset);
            Ok(query
                .build_query_as::<PromptClassObservationRow>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }

    pub async fn get_prompt_class_versions(
        self,
        lane: &str,
        persona: Option<&str>,
    ) -> Result<Option<Vec<PromptClassVersionRow>>, PersistError> {
        crate::metrics::time_db("get_prompt_class_versions", async {
            let rows = sqlx::query_as!(
                PromptClassVersionRow,
                r#"WITH windowed AS MATERIALIZED (
                    SELECT sp.request_id, sp.session_id, sp.created_at, r.model,
                           sp.cc_release AS release, sp.cc_build AS build
                    FROM system_prompts.session_prompts sp
                    LEFT JOIN requests.requests r ON r.id = sp.request_id
                    WHERE sp.lane = $1
                      AND COALESCE(sp.subagent_persona, '') = COALESCE($2::text, '')
                ),
                releases AS (
                    SELECT release,
                           COUNT(DISTINCT build)::bigint AS builds,
                           COUNT(*)::bigint AS observation_count,
                           MIN(created_at) AS first_seen,
                           MAX(created_at) AS last_seen
                    FROM windowed
                    GROUP BY release
                ),
                per_model AS (
                    SELECT release, model,
                           COUNT(*)::bigint AS observation_count,
                           MIN(created_at) AS first_seen,
                           MAX(created_at) AS last_seen
                    FROM windowed
                    GROUP BY release, model
                ),
                latest_per_model AS (
                    SELECT DISTINCT ON (release, model)
                           release, model, request_id, session_id
                    FROM windowed
                    ORDER BY release, model, created_at DESC
                )
                SELECT
                    r.release,
                    r.builds AS "builds!",
                    r.observation_count AS "release_observation_count!",
                    r.first_seen AS "release_first_seen!",
                    r.last_seen AS "release_last_seen!",
                    pm.model,
                    pm.observation_count AS "model_observation_count!",
                    pm.first_seen AS "model_first_seen!",
                    pm.last_seen AS "model_last_seen!",
                    lpm.request_id AS "latest_request_id!",
                    lpm.session_id AS "latest_session_id!"
                FROM releases r
                JOIN per_model pm ON pm.release IS NOT DISTINCT FROM r.release
                JOIN latest_per_model lpm
                    ON lpm.release IS NOT DISTINCT FROM pm.release
                   AND lpm.model IS NOT DISTINCT FROM pm.model
                ORDER BY r.last_seen DESC, pm.observation_count DESC, pm.model"#,
                lane,
                persona,
            )
            .fetch_all(&self.pool)
            .await?;
            if !rows.is_empty() {
                return Ok(Some(rows));
            }
            let registered = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                    SELECT 1 FROM overrides.overrides ov
                    JOIN overrides.sources src ON src.id = ov.source_id AND src.code = 'manual'
                    WHERE ov.lane = $1 AND ov.agent_name IS NOT DISTINCT FROM $2
                ) AS "registered!""#,
                lane,
                persona,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(registered.then(Vec::new))
        })
        .await
    }
}
