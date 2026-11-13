use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionSummaryRow {
    pub session_id: String,
    pub project_path: String,
    pub project_id: Option<i64>,
    pub cwd: Option<String>,
    pub project_resolution: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub request_count: i32,
    pub last_status: Option<String>,
    pub last_models: Vec<String>,
    pub title: Option<String>,
    pub pinned: bool,
    pub project_pinned: bool,
    pub avg_latency_ms: Option<f64>,
    pub originating_operator_prompt: Option<String>,
    pub session_outcome: Option<String>,
    pub session_quality_score: Option<f64>,
    pub session_judged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub overview: Option<String>,
    pub overview_generated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub pending_inbox_count: i32,
    pub projects_touched: Vec<String>,
}

impl SessionSummaryRow {
    pub fn effective_pinned(&self) -> bool {
        self.pinned || self.project_pinned
    }

    pub fn models(&self) -> Vec<String> {
        self.last_models.clone()
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct PromptRosterRow {
    pub prompt_sha256: String,
    pub composition_id: Option<i64>,
    pub first_request_id: i64,
    pub occurrence_count: i64,
    pub label: Option<String>,
}

pub const PROMPT_ROSTER_LIMIT: usize = 100;

pub async fn fetch_session_prompt_roster(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<(Vec<PromptRosterRow>, bool), PersistError> {
    let fetch_size = (PROMPT_ROSTER_LIMIT as i64).saturating_add(1);
    let mut rows = sqlx::query_as!(
        PromptRosterRow,
        r#"SELECT
             agg.prompt_sha256 AS "prompt_sha256!",
             pc.id AS "composition_id?",
             agg.first_request_id AS "first_request_id!",
             agg.occurrence_count AS "occurrence_count!",
             pc.label AS "label?"
         FROM (
             SELECT
                 rsp.prompt_sha256,
                 rsp.override_id,
                 MIN(rsp.request_id)::bigint AS first_request_id,
                 COUNT(*)::bigint              AS occurrence_count
             FROM requests.system_prompts rsp
             JOIN requests.requests r ON r.id = rsp.request_id
             WHERE r.session_id = $1
               AND rsp.prompt_sha256 IS NOT NULL
             GROUP BY rsp.prompt_sha256, rsp.override_id
         ) agg
         LEFT JOIN overrides.overrides pc
             ON pc.id = agg.override_id
         ORDER BY agg.first_request_id ASC
         LIMIT $2"#,
        session_db_id,
        fetch_size,
    )
    .fetch_all(pool)
    .await?;
    let truncated = rows.len() > PROMPT_ROSTER_LIMIT;
    if truncated {
        rows.truncate(PROMPT_ROSTER_LIMIT);
    }
    Ok((rows, truncated))
}

#[derive(Debug, sqlx::FromRow)]
pub struct ModelBreakdownRow {
    pub model: String,
    pub count: i64,
}

pub async fn fetch_session_model_breakdown(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<ModelBreakdownRow>, PersistError> {
    let rows = sqlx::query_as!(
        ModelBreakdownRow,
        r#"SELECT r.model AS "model!", COUNT(*)::bigint AS "count!"
         FROM requests.requests r
         WHERE r.session_id = $1
           AND r.model IS NOT NULL
         GROUP BY r.model
         ORDER BY 2 DESC, r.model ASC"#,
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, sqlx::FromRow)]
pub struct SessionModelTokenUsageRow {
    pub model: String,
    pub response_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

pub async fn fetch_session_token_usage(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<SessionModelTokenUsageRow>, PersistError> {
    let rows = sqlx::query_as!(
        SessionModelTokenUsageRow,
        r#"SELECT r.model AS "model!",
                COUNT(*)::bigint                                           AS "response_count!",
                COALESCE(SUM(resp.input_tokens), 0)::bigint                AS "input_tokens!",
                COALESCE(SUM(resp.output_tokens), 0)::bigint               AS "output_tokens!",
                COALESCE(SUM(resp.cache_read_input_tokens), 0)::bigint     AS "cache_read_tokens!",
                COALESCE(SUM(resp.cache_creation_input_tokens), 0)::bigint AS "cache_write_tokens!"
         FROM requests.requests r
         JOIN responses.responses resp ON resp.request_id = r.id
         WHERE r.session_id = $1
           AND r.model IS NOT NULL
         GROUP BY r.model
         ORDER BY r.model ASC"#,
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
