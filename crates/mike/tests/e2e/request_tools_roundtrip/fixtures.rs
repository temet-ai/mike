use serde_json::Value;
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::anthropic_fixtures;
use anthropic_fixtures::MockResponse;

pub(super) async fn start_upstream_stub() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_rt").minimal().build()),
        )
        .mount(&server)
        .await;
    server
}

pub(super) async fn ensure_parents(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
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

pub(super) async fn seed_request(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    declared: &[&str],
    invoked: &[(&str, &str, Value)],
    offset_ms: i64,
) -> anyhow::Result<(i64, chrono::DateTime<chrono::Utc>)> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(offset_ms);
    let params_seed = serde_json::json!({"thinking_enabled": false});
    let (request_id, request_created_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
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
    .bind(&params_seed)
    .bind(now)
    .fetch_one(pool)
    .await?;

    for (ord, name) in declared.iter().enumerate() {
        sqlx::query(
            "INSERT INTO requests.tools
                (request_id, ord, tool_name)
             VALUES ($1, $2, $3)",
        )
        .bind(request_id)
        .bind(ord as i32)
        .bind(name)
        .execute(pool)
        .await?;
    }

    for (ord, (tool_id, tool_name, input)) in invoked.iter().enumerate() {
        sqlx::query(
            "INSERT INTO tools.uses
                (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             VALUES ($1, $2, $3, $4, $5,
                     (SELECT session_id FROM requests.requests WHERE id = $2))
             ON CONFLICT (wire_tool_use_id) DO NOTHING",
        )
        .bind(tool_id)
        .bind(request_id)
        .bind(ord as i32)
        .bind(tool_name)
        .bind(input)
        .execute(pool)
        .await?;
    }

    Ok((request_id, request_created_at))
}
