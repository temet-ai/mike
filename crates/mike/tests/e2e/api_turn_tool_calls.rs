use crate::db_helpers;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn bootstrap_project_session(
    pool: &PgPool,
    project_path: &str,
    session_id: &str,
) -> anyhow::Result<(i64, i64)> {
    let now = chrono::Utc::now();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, $2, $2)
         RETURNING id",
    )
    .bind(project_path)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let session_pk: i64 = sqlx::query_scalar(
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
    Ok((project_id, session_pk))
}

async fn insert_request(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    created_at_offset_ms: i64,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now() + chrono::Duration::milliseconds(created_at_offset_ms);
    let empty_obj = json!({});
    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id, method_id, path, is_streaming, message_count,
            is_tool_loop, status_id, params,
            last_user_message, human_prompt, turn_type_id, turn_origin_id,
            created_at
         ) VALUES (
            $1, $2, 2, '/v1/messages', false, 0,
            false, 3, $3,
            '', '', 1, 1,
            $4
         ) RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(&empty_obj)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(request_id)
}

struct ToolUseSeed<'a> {
    wire_id: &'a str,
    request_id: i64,
    session_pk: i64,
    ord: i32,
    tool_name: &'a str,
    input: &'a Value,
}

async fn insert_tool_use(pool: &PgPool, seed: ToolUseSeed<'_>) -> anyhow::Result<i64> {
    let tu_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (wire_tool_use_id) DO NOTHING
         RETURNING id",
    )
    .bind(seed.wire_id)
    .bind(seed.request_id)
    .bind(seed.ord)
    .bind(seed.tool_name)
    .bind(seed.input)
    .bind(seed.session_pk)
    .fetch_one(pool)
    .await?;
    Ok(tu_id)
}

async fn insert_tool_result(
    pool: &PgPool,
    tu_id: i64,
    request_id: i64,
    session_pk: i64,
    is_error: bool,
    content: &Value,
) -> anyhow::Result<()> {
    let content_str = serde_json::to_string(content)?;
    let content_sha = format!("{:064x}", content_str.len());
    sqlx::query(
        "INSERT INTO tools.results (tool_use_id, request_id, session_id, ord, is_error, content, content_sha256, content_bytes)
         VALUES ($1, $2, $3, 0, $4, $5, $6, $7)",
    )
    .bind(tu_id)
    .bind(request_id)
    .bind(session_pk)
    .bind(is_error)
    .bind(content)
    .bind(&content_sha)
    .bind(content_str.len() as i64)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_operator_turn(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    start_request_id: i64,
    end_request_id: i64,
) -> anyhow::Result<i64> {
    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.operator_turns (
            session_id, project_id,
            start_request_id, end_request_id,
            outcome_id, classified_at, closed_at,
            user_prompt, final_assistant_text
         ) VALUES ($1, $2, $3, $4,
                   5, NOW(), NOW(),
                   'test prompt', 'test response')
         RETURNING id",
    )
    .bind(session_pk)
    .bind(project_id)
    .bind(start_request_id)
    .bind(end_request_id)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "UPDATE requests.requests SET operator_turn_id = $1
         WHERE id >= $2 AND id <= $3 AND session_id = $4",
    )
    .bind(turn_id)
    .bind(start_request_id)
    .bind(end_request_id)
    .bind(session_pk)
    .execute(pool)
    .await?;
    Ok(turn_id)
}

#[tokio::test]
async fn get_turn_tool_calls_returns_all_calls_across_turn_requests() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "b0000000-0000-4000-8000-000000000001";
    let (project_id, session_pk) =
        bootstrap_project_session(&pool, "test-turn-tool-calls-1", session_id).await?;

    let req1 = insert_request(&pool, session_pk, project_id, 0).await?;
    let req2 = insert_request(&pool, session_pk, project_id, 100).await?;

    let edit_input = json!({"file_path": "/a", "old_string": "x", "new_string": "y"});
    let bash_input = json!({"command": "ls", "description": "list"});
    insert_tool_use(
        &pool,
        ToolUseSeed {
            wire_id: "toolu_turn_edit_01",
            request_id: req1,
            session_pk,
            ord: 1,
            tool_name: "Edit",
            input: &edit_input,
        },
    )
    .await?;
    insert_tool_use(
        &pool,
        ToolUseSeed {
            wire_id: "toolu_turn_bash_02",
            request_id: req1,
            session_pk,
            ord: 2,
            tool_name: "Bash",
            input: &bash_input,
        },
    )
    .await?;

    let write_input = json!({"file_path": "/b", "content": "hello"});
    insert_tool_use(
        &pool,
        ToolUseSeed {
            wire_id: "toolu_turn_write_03",
            request_id: req2,
            session_pk,
            ord: 1,
            tool_name: "Write",
            input: &write_input,
        },
    )
    .await?;

    let turn_id = insert_operator_turn(&pool, session_pk, project_id, req1, req2).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/turns/{turn_id}/tool-calls"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;

    assert_eq!(body["turn_id"].as_i64(), Some(turn_id));
    assert_eq!(body["total_count"].as_u64(), Some(3));

    let calls = body["calls"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("calls must be a JSON array"))?;
    assert_eq!(calls.len(), 3, "three tool calls across two requests");

    assert_eq!(calls[0]["tool_id"].as_str(), Some("toolu_turn_edit_01"));
    assert_eq!(calls[0]["name"].as_str(), Some("Edit"));
    assert_eq!(calls[0]["ord"].as_i64(), Some(1));
    assert_eq!(calls[0]["request_id"].as_i64(), Some(req1));
    assert!(
        calls[0]["input_bytes"].as_i64().unwrap_or(0) > 0,
        "input_bytes must be non-zero for Edit call"
    );
    assert_eq!(
        calls[0]["has_result"].as_bool(),
        Some(false),
        "no result for Edit call"
    );

    assert_eq!(calls[1]["tool_id"].as_str(), Some("toolu_turn_bash_02"));
    assert_eq!(calls[1]["name"].as_str(), Some("Bash"));
    assert_eq!(calls[1]["ord"].as_i64(), Some(2));
    assert_eq!(calls[1]["request_id"].as_i64(), Some(req1));

    assert_eq!(calls[2]["tool_id"].as_str(), Some("toolu_turn_write_03"));
    assert_eq!(calls[2]["name"].as_str(), Some("Write"));
    assert_eq!(calls[2]["ord"].as_i64(), Some(1));
    assert_eq!(calls[2]["request_id"].as_i64(), Some(req2));
    assert!(
        calls[2]["input_bytes"].as_i64().unwrap_or(0) > 0,
        "input_bytes must be non-zero for Write call"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_turn_tool_calls_includes_paired_result() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "b0000000-0000-4000-8000-000000000002";
    let (project_id, session_pk) =
        bootstrap_project_session(&pool, "test-turn-tool-calls-2", session_id).await?;

    let req1 = insert_request(&pool, session_pk, project_id, 0).await?;
    let bash_input = json!({"command": "echo hi"});
    let tu_id = insert_tool_use(
        &pool,
        ToolUseSeed {
            wire_id: "toolu_result_bash_01",
            request_id: req1,
            session_pk,
            ord: 1,
            tool_name: "Bash",
            input: &bash_input,
        },
    )
    .await?;

    let result_content = json!([{"type": "text", "text": "hi\n"}]);
    insert_tool_result(&pool, tu_id, req1, session_pk, false, &result_content).await?;

    let turn_id = insert_operator_turn(&pool, session_pk, project_id, req1, req1).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/turns/{turn_id}/tool-calls"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;

    assert_eq!(body["total_count"].as_u64(), Some(1));
    let calls = body["calls"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("calls must be a JSON array"))?;
    let call = &calls[0];

    assert_eq!(
        call["has_result"].as_bool(),
        Some(true),
        "has_result must be true when a result exists"
    );
    assert_eq!(call["result_is_error"].as_bool(), Some(false));
    assert!(
        call["result_content_bytes"].as_u64().unwrap_or(0) > 0,
        "result_content_bytes must be non-zero"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_turn_tool_calls_unknown_session_returns_404() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, _pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/ffffffff-ffff-4fff-8fff-ffffffffffff/turns/1/tool-calls"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_turn_tool_calls_unknown_turn_returns_404() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "b0000000-0000-4000-8000-000000000003";
    bootstrap_project_session(&pool, "test-turn-tool-calls-3", session_id).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/turns/999999999/tool-calls"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
