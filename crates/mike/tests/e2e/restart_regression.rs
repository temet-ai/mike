use crate::anthropic_fixtures;
use crate::db_helpers;

use anthropic_fixtures::{
    MockResponse, RequestBody, assistant_message, tool_use_block, user_message,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn seed_emitting_request_with_three_tool_uses(
    pool: &PgPool,
    session_id: &str,
    project_path: &str,
    tool_ids_and_names: &[(&str, &str)],
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

    let params_seed = json!({"thinking_enabled": false});
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
    .bind(&params_seed)
    .bind(now)
    .fetch_one(pool)
    .await?;

    for (ord, (tool_id, tool_name)) in tool_ids_and_names.iter().enumerate() {
        sqlx::query(
            "INSERT INTO tools.uses
                (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
             VALUES ($1, $2, $3, $4, '{}'::jsonb, $5)
             ON CONFLICT (wire_tool_use_id) DO NOTHING",
        )
        .bind(*tool_id)
        .bind(request_id)
        .bind(ord as i32)
        .bind(*tool_name)
        .bind(session_row_id)
        .execute(pool)
        .await?;
    }

    Ok(request_id)
}

async fn start_upstream_stub() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(MockResponse::new("msg_c15").minimal().build()),
        )
        .mount(&server)
        .await;
    server
}

fn carrying_body_with_tool_result(session_id: &str, tool_use_id: &str, content: Value) -> Value {
    let mut body = RequestBody::default()
        .messages(json!([
            user_message("next turn"),
            assistant_message(json!([tool_use_block(tool_use_id, "Bash", json!({}))])),
            user_message(json!([{
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "is_error": false,
                "content": content
            }])),
        ]))
        .build();
    body["session_id_hint"] = json!(session_id);
    body
}

async fn fetch_results_via_api(
    client: &reqwest::Client,
    inspect_url: &str,
    emitter_id: i64,
) -> anyhow::Result<Vec<(String, Value)>> {
    let resp = client
        .get(format!(
            "{inspect_url}/api/requests/{emitter_id}/tool-results"
        ))
        .send()
        .await?;
    anyhow::ensure!(
        resp.status() == 200,
        "GET /api/requests/{emitter_id}/tool-results returned {}",
        resp.status()
    );
    let body: Value = resp.json().await?;
    let arr = body
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("results must be an array: {body}"))?;
    Ok(arr
        .iter()
        .map(|r| {
            (
                r.get("tool_use_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                r.get("content").cloned().unwrap_or(Value::Null),
            )
        })
        .collect())
}

#[tokio::test]
async fn tool_use_matcher_works_after_restart() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping C15 restart regression");
        return Ok(());
    };
    let (proxy_url_1, inspect_url_1, shutdown_1, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655c15001";
    let project_path = "test-project-c15-restart-matcher";

    let tools = [
        ("toolu_c15_bash_001", "Bash"),
        ("toolu_c15_edit_002", "Edit"),
        ("toolu_c15_write_003", "Write"),
    ];

    let emitting_id =
        seed_emitting_request_with_three_tool_uses(&pool, session_id, project_path, &tools).await?;

    let pre: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tools.results")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        pre.0, 0,
        "tool_results must start empty before any carrier lands"
    );

    let client = reqwest::Client::new();
    let mut expected: Vec<(String, Value)> = Vec::new();
    for (tool_use_id, _tool_name) in &tools {
        let content = json!(format!("result body for {tool_use_id}"));
        let body = carrying_body_with_tool_result(session_id, tool_use_id, content.clone());
        let resp = client
            .post(format!("{proxy_url_1}/v1/messages"))
            .header("x-claude-code-session-id", session_id)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        assert!(
            resp.status().is_success(),
            "carrier for {tool_use_id} failed: {}",
            resp.status()
        );
        expected.push((tool_use_id.to_string(), content));
    }

    let pre_results = fetch_results_via_api(&client, &inspect_url_1, emitting_id).await?;
    assert_eq!(
        pre_results.len(),
        3,
        "pre-restart: expected 3 results from emitter /tool-results, got {}",
        pre_results.len(),
    );
    assert_eq!(
        pre_results, expected,
        "pre-restart: tool_use_id + content must round-trip exactly"
    );

    shutdown_1.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let database_url = test_db.database_url.clone();
    let (_proxy_url_2, inspect_url_2, shutdown_2) =
        db_helpers::start_proxy_with_db_at(&upstream.uri(), &database_url).await?;

    let post_results = fetch_results_via_api(&client, &inspect_url_2, emitting_id).await?;
    assert_eq!(
        post_results.len(),
        3,
        "post-restart: matcher must surface all 3 results via the FK; got {}. \
         If this is 0 the matcher regressed to in-memory state; if it is <3 some carriers \
         lost their write under restart.",
        post_results.len(),
    );
    assert_eq!(
        post_results, expected,
        "post-restart: tool_use_id + content must round-trip identically across the restart"
    );

    let row_request_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT tr.request_id
           FROM tools.results tr
           JOIN tools.uses    tu ON tu.id = tr.tool_use_id
          WHERE tu.request_id = $1
          ORDER BY tr.request_id ASC",
    )
    .bind(emitting_id)
    .fetch_all(&pool)
    .await?;
    let unique_carrier_ids: std::collections::BTreeSet<i64> =
        row_request_ids.iter().copied().collect();
    assert_eq!(
        unique_carrier_ids.len(),
        3,
        "post-restart: each carrier must own a distinct request_id; got {unique_carrier_ids:?}",
    );
    assert!(
        !unique_carrier_ids.contains(&emitting_id),
        "post-restart: no row may live under the emitting request's id (C7 invariant)"
    );

    shutdown_2.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
