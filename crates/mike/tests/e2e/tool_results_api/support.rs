use crate::anthropic_fixtures;

use anthropic_fixtures::{
    MockResponse, RequestBody, assistant_message, tool_use_block, user_message,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(crate) async fn insert_emitting_request(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
    tool_id: &str,
    tool_name: &str,
    created_at_offset_ms: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(created_at_offset_ms);
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         SELECT $1, $1, $2, $2
         WHERE NOT EXISTS (SELECT 1 FROM projects.projects WHERE project_path = $1)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_optional(pool)
    .await?
    .unwrap_or(0i64);
    let project_id: i64 = if project_id == 0 {
        sqlx::query_scalar("SELECT id FROM projects.projects WHERE project_path = $1")
            .bind(project_path)
            .fetch_one(pool)
            .await?
    } else {
        project_id
    };

    let session_row_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, $3, $3)
         ON CONFLICT (session_uuid) DO UPDATE SET last_seen = EXCLUDED.last_seen
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(now)
    .fetch_one(pool)
    .await?;

    let empty_obj = json!({});
    let (request_id, _request_created_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            $1, $2, 2, '/v1/messages', false, 0, false,
            3, $3,
            '', '', 1, 1,
            $4
         ) RETURNING id, created_at",
    )
    .bind(session_row_id)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(now)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, 0, $3, '{}'::jsonb, $4)
         ON CONFLICT (wire_tool_use_id) DO NOTHING",
    )
    .bind(tool_id)
    .bind(request_id)
    .bind(tool_name)
    .bind(session_row_id)
    .execute(pool)
    .await?;

    Ok(request_id)
}

pub(crate) fn follow_up_body_with_tool_result(
    session_id: &str,
    tool_use_id: &str,
    is_error: bool,
    content: Value,
) -> Value {
    let mut body = RequestBody::default()
        .messages(json!([
            user_message("do the thing"),
            assistant_message(json!([tool_use_block(tool_use_id, "Bash", json!({}))])),
            user_message(json!([{
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "is_error": is_error,
                "content": content
            }])),
        ]))
        .build();
    body["session_id_hint"] = json!(session_id);
    body
}

pub(crate) async fn start_upstream_stub() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_test").minimal().build()),
        )
        .mount(&server)
        .await;
    server
}
