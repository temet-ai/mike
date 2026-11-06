use serde_json::{Value, json};
use sqlx::PgPool;

pub(crate) async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, $2, $2
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects.projects WHERE project_path = $1
             )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3) ON CONFLICT (session_uuid) DO NOTHING",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(project_id)
}

pub(crate) struct InsertRequestParams<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) project_id: i64,
    pub(crate) turn_origin: &'a str,
    pub(crate) turn_type: &'a str,
    pub(crate) human_prompt: &'a str,
    pub(crate) created_at_offset_ms: i64,
}

pub(crate) async fn insert_request(
    pool: &PgPool,
    p: InsertRequestParams<'_>,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(p.created_at_offset_ms);
    let empty_obj = json!({});
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2,
            (SELECT id FROM requests.methods WHERE code = 'POST'),
            '/v1/messages', false, 0, false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            $3,
            '', $4,
            (SELECT id FROM requests.turn_types WHERE code = $5),
            (SELECT id FROM requests.turn_origins WHERE code = $6),
            $7, $7
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(&empty_obj)
    .bind(p.human_prompt)
    .bind(p.turn_type)
    .bind(p.turn_origin)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub(crate) struct InsertOperatorTurnParams<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) project_id: i64,
    pub(crate) start_request_id: i64,
    pub(crate) end_request_id: i64,
    pub(crate) user_prompt: &'a str,
    pub(crate) final_assistant_text: &'a str,
    pub(crate) files_touched: &'a Value,
    pub(crate) outcome: &'a str,
    pub(crate) outcome_evidence: Option<&'a str>,
}

pub(crate) async fn insert_operator_turn(
    pool: &PgPool,
    p: InsertOperatorTurnParams<'_>,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
    let empty_obj = json!({});
    let (req_id, req_ca): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count,
            is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at, completed_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2,
            (SELECT id FROM requests.methods WHERE code = 'POST'),
            '/v1/messages', false, 0,
            false,
            (SELECT id FROM requests.statuses WHERE code = 'complete'),
            $3,
            $4, $4,
            (SELECT id FROM requests.turn_types WHERE code = 'user_prompt'),
            (SELECT id FROM requests.turn_origins WHERE code = 'primary'),
            $5, $5
         RETURNING id, created_at",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(&empty_obj)
    .bind(p.user_prompt)
    .bind(now)
    .fetch_one(pool)
    .await?;
    if let Some(arr) = p.files_touched.as_array() {
        for (i, f) in arr.iter().enumerate() {
            if let Some(path) = f.get("path").and_then(|v| v.as_str()) {
                sqlx::query(
                    "INSERT INTO tools.uses
                        (wire_tool_use_id, request_id, ord,
                         tool_name, input, file_path, created_at, session_id)
                     VALUES ($1, $2, $3, 'Edit', $4, $5, $6,
                             (SELECT session_id FROM requests.requests WHERE id = $2))",
                )
                .bind(format!("toolu_{req_id}_{i}"))
                .bind(req_id)
                .bind(i as i32)
                .bind(json!({ "file_path": path }))
                .bind(path)
                .bind(req_ca)
                .execute(pool)
                .await?;
            }
        }
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            outcome_id, classified_at, closed_at,
            user_prompt, final_assistant_text
         ) VALUES (
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2, $3, $3,
            (SELECT id FROM memory.turn_outcomes WHERE code = $4),
            NOW(), NOW(),
            $5, $6
         )
         RETURNING id",
    )
    .bind(p.session_id)
    .bind(p.project_id)
    .bind(req_id)
    .bind(p.outcome)
    .bind(p.user_prompt)
    .bind(p.final_assistant_text)
    .fetch_one(pool)
    .await?;
    let _ = (&p.start_request_id, &p.end_request_id, &p.outcome_evidence);
    Ok(id)
}
