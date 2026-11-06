use anthropic_fixtures::MockResponse;
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::anthropic_fixtures;

pub(crate) async fn start_upstream_stub(msg_id: &str) -> MockServer {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new(msg_id).minimal().build()),
        )
        .mount(&upstream)
        .await;
    upstream
}

pub(crate) async fn ensure_parents(
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

pub(crate) async fn seed_request_with_body(
    pool: &PgPool,
    session_id: &str,
    project_id: i64,
    body: &[u8],
    offset_ms: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(offset_ms);
    let params_seed = serde_json::json!({"thinking_enabled": false});
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
            status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            request_body,
            created_at
         )
         SELECT
            (SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid),
            $2, rm.id, '/v1/messages', false, 0, false,
            rs.id, $3,
            '', '', rtt.id, rto.id,
            $5,
            $4
         FROM requests.methods rm, requests.statuses rs,
              requests.turn_types rtt, requests.turn_origins rto
         WHERE rm.code = 'POST' AND rs.code = 'complete'
           AND rtt.code = 'user_prompt' AND rto.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .bind(&params_seed)
    .bind(now)
    .bind(body)
    .fetch_one(pool)
    .await?;

    Ok(request_id)
}
