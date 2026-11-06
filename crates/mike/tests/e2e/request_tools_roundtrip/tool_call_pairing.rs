use crate::db_helpers;

use super::fixtures::{ensure_parents, seed_request, start_upstream_stub};

use serde_json::{Value, json};

async fn seed_emitter_with_two_calls(
    pool: &sqlx::PgPool,
    session_id: &str,
    project_id: i64,
) -> anyhow::Result<(i64, Value, Value)> {
    let input_a = json!({"command": "ls -la /tmp"});
    let input_b = json!({"path": "/etc/hosts"});
    let (emitter_id, _emitter_created) = seed_request(
        pool,
        session_id,
        project_id,
        &[],
        &[
            ("toolu_A", "Bash", input_a.clone()),
            ("toolu_B", "Read", input_b.clone()),
        ],
        0,
    )
    .await?;
    Ok((emitter_id, input_a, input_b))
}

async fn seed_result_for_toolu_a(
    pool: &sqlx::PgPool,
    session_id: &str,
    project_id: i64,
) -> anyhow::Result<Value> {
    let (carrier_id, carrier_created) =
        seed_request(pool, session_id, project_id, &[], &[], 10).await?;
    let result_content = json!("total 0\ndrwxr-xr-x  2 root root  40 tmp");
    let session_pk: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session_id)
            .fetch_one(pool)
            .await?;
    sqlx::query(
        "INSERT INTO tools.results (
            request_id, ord, tool_use_id, session_id,
            is_error, content, content_sha256, content_bytes, created_at
         ) VALUES ($1, 0,
            (SELECT id FROM tools.uses WHERE wire_tool_use_id = 'toolu_A'),
            $4,
            false, $2, 'sha-paired', 42, $3)",
    )
    .bind(carrier_id)
    .bind(&result_content)
    .bind(carrier_created)
    .bind(session_pk)
    .execute(pool)
    .await?;
    Ok(result_content)
}

fn assert_call_a_answered(
    c0: &Value,
    input_a: &Value,
    result_content: &Value,
) -> anyhow::Result<()> {
    assert_eq!(c0.get("tool_id").and_then(Value::as_str), Some("toolu_A"));
    assert_eq!(c0.get("name").and_then(Value::as_str), Some("Bash"));
    assert_eq!(
        c0.get("input"),
        Some(input_a),
        "full structured input, not double-encoded, not sliced"
    );
    let r0 = c0
        .get("result")
        .ok_or_else(|| anyhow::anyhow!("calls[0].result key absent"))?;
    assert!(
        !r0.is_null(),
        "toolu_A was answered — result must be present"
    );
    assert_eq!(
        r0.get("tool_use_id").and_then(Value::as_str),
        c0.get("tool_id").and_then(Value::as_str),
        "result.tool_use_id must equal the call's tool_id"
    );
    assert_eq!(
        r0.get("content"),
        Some(result_content),
        "full result content round-trips verbatim"
    );
    Ok(())
}

fn assert_call_b_unanswered(c1: &Value, input_b: &Value) {
    assert_eq!(c1.get("tool_id").and_then(Value::as_str), Some("toolu_B"));
    assert_eq!(c1.get("input"), Some(input_b));
    assert!(
        c1.get("result").map(Value::is_null).unwrap_or(false),
        "unanswered toolu_B must surface result: null, never a dropped row; got {:?}",
        c1.get("result")
    );
}

#[tokio::test]
async fn paired_endpoint_links_call_to_result() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping request_tools_roundtrip");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-44665544aa03";
    let project_path = "test-project-rt-paired";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    let (emitter_id, input_a, input_b) =
        seed_emitter_with_two_calls(&pool, session_id, project_id).await?;
    let result_content = seed_result_for_toolu_a(&pool, session_id, project_id).await?;

    let body: Value = reqwest::Client::new()
        .get(format!(
            "{inspect_url}/api/requests/{emitter_id}/tool-calls"
        ))
        .send()
        .await?
        .json()
        .await?;

    assert_eq!(
        body.get("request_id").and_then(Value::as_u64),
        Some(emitter_id as u64),
        "envelope request_id must be the emitter"
    );
    assert_eq!(
        body.get("total_count").and_then(Value::as_u64),
        Some(2),
        "two emitted calls => total_count 2"
    );
    let calls = body
        .get("calls")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("calls must be an array"))?;
    assert_eq!(
        calls.len(),
        2,
        "two emitted calls, both rows present (LEFT JOIN)"
    );

    assert_call_a_answered(&calls[0], &input_a, &result_content)?;
    assert_call_b_unanswered(&calls[1], &input_b);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn declared_vs_invoked_not_conflated() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping request_tools_roundtrip");
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-44665544aa04";
    let project_path = "test-project-rt-conflate";
    let project_id = ensure_parents(&pool, session_id, project_path).await?;

    seed_request(
        &pool,
        session_id,
        project_id,
        &["Read"],
        &[
            ("toolu_b1", "Bash", json!({})),
            ("toolu_b2", "Bash", json!({})),
        ],
        0,
    )
    .await?;

    let detail: Value = reqwest::Client::new()
        .get(format!("{inspect_url}/api/sessions/{session_id}"))
        .send()
        .await?
        .json()
        .await?;
    let requests = detail
        .get("requests")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("SessionDetail.requests must be an array"))?;
    let row = requests
        .first()
        .ok_or_else(|| anyhow::anyhow!("session listing must contain the seeded request"))?;

    let tool_names: Vec<String> = serde_json::from_value(
        row.get("tool_names")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("RequestSummary.tool_names missing"))?,
    )?;
    let response_tools_used: Vec<String> = serde_json::from_value(
        row.get("response_tools_used")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("RequestSummary.response_tools_used missing"))?,
    )?;

    assert_eq!(
        tool_names,
        vec!["Read"],
        "tool_names must be the DECLARED set (request_tools), not invoked"
    );
    assert_eq!(
        response_tools_used,
        vec!["Bash"],
        "response_tools_used must be the INVOKED set (tool_uses), deduped"
    );
    assert_eq!(
        row.get("response_tool_count").and_then(Value::as_u64),
        Some(2),
        "response_tool_count counts invocations (2 Bash calls), not declared"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
