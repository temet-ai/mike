use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{
    MockResponse, RequestBody, assistant_message, tool_use_block, user_message,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn seed_emitting_request_only_in_db(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
    tool_id: &str,
    tool_name: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now();
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
    let session_row_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session_id)
            .fetch_one(pool)
            .await?;

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

async fn start_upstream_stub() -> MockServer {
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

fn follow_up_body_with_tool_result(
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

#[allow(dead_code)]
async fn fetch_tool_results_jsonb(
    pool: &PgPool,
    emitting_request_id: i64,
) -> anyhow::Result<Value> {
    let row: (Option<Value>,) = sqlx::query_as(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(
                    'tool_use_id',     tu.wire_tool_use_id,
                    'is_error',        tr.is_error,
                    'content_sha256',  tr.content_sha256,
                    'content_bytes',   tr.content_bytes,
                    'content',         tr.content
                ) ORDER BY tr.ord), '[]'::jsonb)
           FROM tools.results tr
           JOIN tools.uses    tu ON tu.id = tr.tool_use_id
          WHERE tu.request_id = $1",
    )
    .bind(emitting_request_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0.unwrap_or_else(|| serde_json::json!([])))
}

#[tokio::test]
async fn db_fallback_attaches_tool_result_when_emitting_request_is_only_in_postgres()
-> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655441001";
    let project_path = "test-project-post-restart-fallback";
    let tool_use_id = "toolu_01postrestart";

    let emitting_id =
        seed_emitting_request_only_in_db(&pool, session_id, project_path, tool_use_id, "Bash")
            .await?;

    let pre = fetch_tool_results_jsonb(&pool, emitting_id).await?;
    assert_eq!(
        pre,
        json!([]),
        "seeded row must start with empty tool_results"
    );

    let result_content = json!("post-restart result body that must round-trip verbatim");
    let follow_up =
        follow_up_body_with_tool_result(session_id, tool_use_id, false, result_content.clone());

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-claude-code-session-id", session_id)
        .header("content-type", "application/json")
        .json(&follow_up)
        .send()
        .await?;
    assert!(
        resp.status().is_success(),
        "proxy roundtrip failed: {}",
        resp.status()
    );

    let post = fetch_tool_results_jsonb(&pool, emitting_id).await?;
    let arr = post
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tool_results must be an array"))?;
    assert_eq!(
        arr.len(),
        1,
        "DB-fallback path must append exactly one result; got {arr:?}"
    );
    assert_eq!(arr[0]["tool_use_id"].as_str(), Some(tool_use_id));
    assert_eq!(arr[0]["is_error"].as_bool(), Some(false));
    assert_eq!(
        arr[0]["content"], result_content,
        "full content must round-trip byte-identical (inspection-fidelity)"
    );
    assert_eq!(
        arr[0]["content_sha256"].as_str().map(str::len).unwrap_or(0),
        64,
        "content_sha256 must be a 64-char hex string"
    );
    assert!(
        arr[0]["content_bytes"].as_u64().unwrap_or(0) > 0,
        "content_bytes must be populated"
    );

    let resp = client
        .get(format!(
            "{inspect_url}/api/requests/{emitting_id}/tool-results"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(body["request_id"].as_u64(), Some(emitting_id as u64));
    assert_eq!(body["total_count"].as_u64(), Some(1));
    let results = body["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("results must be an array"))?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["tool_use_id"].as_str(), Some(tool_use_id));
    assert_eq!(
        results[0]["content"], result_content,
        "inspect API must return the full content captured via DB fallback"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn db_fallback_each_replay_creates_distinct_tool_results_row() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655441002";
    let project_path = "test-project-post-restart-replay";
    let tool_use_id = "toolu_02postrestart";

    let emitting_id =
        seed_emitting_request_only_in_db(&pool, session_id, project_path, tool_use_id, "Edit")
            .await?;

    let result_content = json!("replayed body");
    let follow_up =
        follow_up_body_with_tool_result(session_id, tool_use_id, false, result_content.clone());

    let client = reqwest::Client::new();

    for _ in 0..3 {
        let resp = client
            .post(format!("{proxy_url}/v1/messages"))
            .header("x-claude-code-session-id", session_id)
            .header("content-type", "application/json")
            .json(&follow_up)
            .send()
            .await?;
        assert!(resp.status().is_success());
    }

    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT tr.request_id, tu.wire_tool_use_id, tr.content_sha256
           FROM tools.results tr
           JOIN tools.uses    tu ON tu.id = tr.tool_use_id
          WHERE tu.request_id = $1
          ORDER BY tr.request_id ASC",
    )
    .bind(emitting_id)
    .fetch_all(&pool)
    .await?;

    assert_eq!(
        rows.len(),
        1,
        "UNIQUE(tool_use_id): three replays must produce exactly ONE stored result row \
         (first wins, subsequent ON CONFLICT DO NOTHING); got {}",
        rows.len(),
    );

    assert_eq!(
        rows[0].1.as_str(),
        tool_use_id,
        "stored row must reference the emitter's tool_use_id"
    );
    assert_eq!(
        rows[0].2.len(),
        64,
        "content_sha256 must be a 64-char hex string"
    );

    assert!(
        rows[0].0 != emitting_id,
        "the stored row must carry the first replay's request id, not the emitter's; \
         got carrier id {} == emitting id {}",
        rows[0].0,
        emitting_id,
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
