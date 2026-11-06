use serde_json::{Value, json};
use sqlx::PgPool;

pub(super) async fn insert_request_with_tool_calls(
    pool: &PgPool,
    session_id: &str,
    tool_calls: &Value,
    created_at_offset_ms: i64,
) -> anyhow::Result<i64> {
    let project_path = format!("test-project-{session_id}");
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(created_at_offset_ms);
    sqlx::query(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
         WHERE NOT EXISTS (
             SELECT 1 FROM projects.projects WHERE project_path = $1
         )",
    )
    .bind(&project_path)
    .bind(now)
    .execute(pool)
    .await?;
    let project_id: i64 = sqlx::query_scalar(
        "SELECT id FROM projects.projects WHERE project_path = $1 ORDER BY id LIMIT 1",
    )
    .bind(&project_path)
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

    let empty_obj = json!({});
    let (request_id, _request_created_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         )
         SELECT (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
                $2, rm.id, '/v1/messages', false, 0, false,
                rs.id, $3,
                '', '', rtt.id, rto.id,
                $4
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id, created_at",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(now)
    .fetch_one(pool)
    .await?;

    if let Some(arr) = tool_calls.as_array() {
        for (ord, call) in arr.iter().enumerate() {
            let tool_id = call
                .get("tool_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("tool_calls[{ord}].tool_id missing"))?;
            let name = call
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let input_json = call
                .get("input_json")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let input_value: Option<Value> = if input_json.is_empty() {
                None
            } else {
                serde_json::from_str(input_json).ok()
            };
            let file_path: Option<&str> = input_value
                .as_ref()
                .and_then(|v| v.get("file_path"))
                .and_then(|v| v.as_str());
            sqlx::query(
                "INSERT INTO tools.uses
                    (wire_tool_use_id, request_id, ord, tool_name, input, file_path, session_id)
                 VALUES ($1, $2, $3, $4, $5, $6,
                         (SELECT session_id FROM requests.requests WHERE id = $2))
                 ON CONFLICT (wire_tool_use_id) DO NOTHING",
            )
            .bind(tool_id)
            .bind(request_id)
            .bind((ord + 1) as i32)
            .bind(name)
            .bind(input_value.as_ref())
            .bind(file_path)
            .execute(pool)
            .await?;
        }
    }
    Ok(request_id)
}

pub(super) fn canonical_tool_calls() -> anyhow::Result<(Value, Value, Value, Value)> {
    let edit_input = json!({"file_path": "/a", "old_string": "foo", "new_string": "bar"});
    let bash_input = json!({"command": "x", "description": "exec x"});
    let write_input = json!({"file_path": "/b", "content": "hello"});
    let tool_calls = json!([
        {"tool_id": "toolu_edit_001",  "name": "Edit",  "input_json": serde_json::to_string(&edit_input)?,  "complete": true},
        {"tool_id": "toolu_bash_002",  "name": "Bash",  "input_json": serde_json::to_string(&bash_input)?,  "complete": true},
        {"tool_id": "toolu_write_003", "name": "Write", "input_json": serde_json::to_string(&write_input)?, "complete": true},
    ]);
    Ok((tool_calls, edit_input, bash_input, write_input))
}

pub(super) async fn insert_subagent_tool_call(
    pool: &PgPool,
    session_id: &str,
    persona: &str,
    tool_id: &str,
    created_at_offset_ms: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(created_at_offset_ms);
    let (request_id, session_db_id): (i64, i64) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
            subagent_persona, created_at
         )
         SELECT s.id, s.project_id, rm.id, '/v1/messages', false, 0, false,
                rs.id, '{}'::jsonb, '', '', rtt.id, rto.id,
                $2, $3
           FROM sessions.sessions s,
                requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.session_uuid = $1::text::uuid
            AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'subagent'
         RETURNING id, session_id",
    )
    .bind(session_id)
    .bind(persona)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, 1, 'Bash', '{}'::jsonb, $3)",
    )
    .bind(tool_id)
    .bind(request_id)
    .bind(session_db_id)
    .execute(pool)
    .await?;

    Ok(request_id)
}
