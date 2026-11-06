use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::super::PersistError;

#[derive(Debug)]
pub(crate) struct DeliveredLessonRow {
    pub(crate) lesson_id: i64,
    pub(crate) text: String,
    pub(crate) kind: String,
    pub(crate) topic: Option<String>,
    pub(crate) stage: String,
    pub(crate) archived_at: Option<DateTime<Utc>>,
    pub(crate) archived_reason: Option<String>,
    pub(crate) injection_count: i64,
    pub(crate) first_injected_at: DateTime<Utc>,
    pub(crate) last_injected_at: DateTime<Utc>,
    pub(crate) first_request_id: i64,
    pub(crate) last_request_id: i64,
    pub(crate) lanes: Vec<String>,
    pub(crate) personas: Vec<String>,
    pub(crate) origins: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct AvailableTopicRow {
    pub(crate) topic: String,
    pub(crate) shared: bool,
    pub(crate) lesson_count: i64,
}

#[derive(Debug)]
pub(crate) struct DeliveryStageRow {
    pub(crate) stage: String,
    pub(crate) outcome: String,
    pub(crate) reason_code: String,
    pub(crate) request_count: i64,
    pub(crate) first_request_id: i64,
    pub(crate) last_request_id: i64,
}

#[derive(Debug)]
pub(crate) struct DeliveryRecallRow {
    pub(crate) request_id: i64,
    pub(crate) recalled_at: DateTime<Utc>,
    pub(crate) topics: serde_json::Value,
    pub(crate) lesson_count: Option<i64>,
}

#[derive(Debug)]
pub(crate) struct DeliveryToolUseRow {
    pub(crate) tool_name: String,
    pub(crate) skill: Option<String>,
    pub(crate) use_count: i64,
    pub(crate) error_count: i64,
    pub(crate) first_request_id: i64,
    pub(crate) last_request_id: i64,
    pub(crate) first_used_at: DateTime<Utc>,
    pub(crate) last_used_at: DateTime<Utc>,
}

pub(super) async fn delivered_lessons(
    pool: &PgPool,
    session_id: i64,
) -> Result<Vec<DeliveredLessonRow>, PersistError> {
    let rows = sqlx::query_as!(
        DeliveredLessonRow,
        r#"SELECT l.id AS lesson_id,
                  l.text,
                  lk.code AS kind,
                  l.topic,
                  ls.code AS stage,
                  l.archived_at,
                  ar.code AS "archived_reason?",
                  count(*) AS "injection_count!",
                  min(li.injected_at) AS "first_injected_at!",
                  max(li.injected_at) AS "last_injected_at!",
                  min(li.request_id) AS "first_request_id!",
                  max(li.request_id) AS "last_request_id!",
                  array_agg(DISTINCT lane.code) AS "lanes!: Vec<String>",
                  array_remove(array_agg(DISTINCT li.persona), NULL) AS "personas!: Vec<String>",
                  array_agg(DISTINCT li.origin ORDER BY li.origin) AS "origins!: Vec<String>"
             FROM memory.lesson_injections li
             JOIN memory.lessons l ON l.id = li.lesson_id
             JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
             JOIN memory.lesson_stages ls ON ls.id = l.stage_id
             JOIN memory.lesson_injection_lanes lane ON lane.id = li.lane_id
        LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
            WHERE li.session_id = $1
            GROUP BY l.id, l.text, lk.code, l.topic, ls.code, l.archived_at, ar.code
            ORDER BY min(li.injected_at), l.id"#,
        session_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub(super) async fn attributed_lesson_requests(
    pool: &PgPool,
    session_id: i64,
) -> Result<i64, PersistError> {
    let count = sqlx::query_scalar!(
        r#"SELECT count(DISTINCT li.request_id) AS "count!"
             FROM memory.lesson_injections li
            WHERE li.session_id = $1 AND li.origin = 'proxy'"#,
        session_id
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

pub(super) async fn available_topics(
    pool: &PgPool,
    project_id: i64,
) -> Result<Vec<AvailableTopicRow>, PersistError> {
    let rows = sqlx::query_as!(
        AvailableTopicRow,
        r#"SELECT l.topic AS "topic!",
                  (l.project_id IS NULL) AS "shared!",
                  count(*) AS "lesson_count!"
             FROM memory.lessons l
            WHERE l.topic IS NOT NULL
              AND l.archived_at IS NULL
              AND l.stage_id IN (2, 3)
              AND (l.expires_at IS NULL OR l.expires_at > now())
              AND (l.project_id = $1 OR l.project_id IS NULL)
            GROUP BY l.topic, (l.project_id IS NULL)
            ORDER BY l.topic, 2"#,
        project_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub(super) async fn stages(
    pool: &PgPool,
    session_id: i64,
) -> Result<Vec<DeliveryStageRow>, PersistError> {
    let rows = sqlx::query_as!(
        DeliveryStageRow,
        r#"SELECT st.stage->>'stage' AS "stage!",
                  st.stage->>'outcome' AS "outcome!",
                  st.stage->>'reason_code' AS "reason_code!",
                  count(*) AS "request_count!",
                  min(r.id) AS "first_request_id!",
                  max(r.id) AS "last_request_id!"
             FROM requests.requests r,
                  jsonb_array_elements(r.transformation_manifest->'stages') AS st(stage)
            WHERE r.session_id = $1
            GROUP BY 1, 2, 3
            ORDER BY min(r.id), 1, 2, 3"#,
        session_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub(super) async fn recalls(
    pool: &PgPool,
    session_id: i64,
) -> Result<Vec<DeliveryRecallRow>, PersistError> {
    let rows = sqlx::query_as!(
        DeliveryRecallRow,
        r#"SELECT r.id AS request_id,
                  r.created_at AS recalled_at,
                  COALESCE(st.stage->'resolved_parameters'->'topics', '[]'::jsonb) AS "topics!: serde_json::Value",
                  (st.stage->'resolved_parameters'->>'lesson_count')::bigint AS lesson_count
             FROM requests.requests r,
                  jsonb_array_elements(r.transformation_manifest->'stages') AS st(stage)
            WHERE r.session_id = $1
              AND st.stage->>'stage' = 'recall_injector'
              AND st.stage->>'outcome' = 'applied'
            ORDER BY r.id"#,
        session_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub(super) async fn tool_uses(
    pool: &PgPool,
    session_id: i64,
    recall_tools: &[String],
) -> Result<Vec<DeliveryToolUseRow>, PersistError> {
    let rows = sqlx::query_as!(
        DeliveryToolUseRow,
        r#"SELECT tu.tool_name,
                  NULLIF(tu.input->>'skill', '') AS skill,
                  count(*) AS "use_count!",
                  count(*) FILTER (WHERE tr.is_error) AS "error_count!",
                  min(tu.request_id) AS "first_request_id!",
                  max(tu.request_id) AS "last_request_id!",
                  min(tu.created_at) AS "first_used_at!",
                  max(tu.created_at) AS "last_used_at!"
             FROM tools.uses tu
        LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
            WHERE tu.session_id = $1
              AND (tu.tool_name = 'Skill' OR tu.tool_name = ANY($2))
            GROUP BY tu.tool_name, NULLIF(tu.input->>'skill', '')
            ORDER BY min(tu.created_at), tu.tool_name, 2"#,
        session_id,
        recall_tools
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
