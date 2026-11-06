use crate::anthropic_fixtures;
use crate::db_helpers;
use crate::sse;
use crate::upstream;

use std::time::Duration;

use anthropic_fixtures::RequestBody;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::Row;
use sse::sse_body;

fn message_start_event(message_id: &str) -> anyhow::Result<(&'static str, String)> {
    Ok((
        "message_start",
        serde_json::to_string(&json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 12, "output_tokens": 0}
            }
        }))?,
    ))
}

fn tool_use_block_events(
    index: u32,
    id: &str,
    name: &str,
    input: &Value,
) -> anyhow::Result<Vec<(&'static str, String)>> {
    Ok(vec![
        (
            "content_block_start",
            serde_json::to_string(&json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": {}
                }
            }))?,
        ),
        (
            "content_block_delta",
            serde_json::to_string(&json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": serde_json::to_string(input)?
                }
            }))?,
        ),
        (
            "content_block_stop",
            serde_json::to_string(&json!({"type": "content_block_stop", "index": index}))?,
        ),
    ])
}

fn terminal_tool_use_events() -> anyhow::Result<Vec<(&'static str, String)>> {
    Ok(vec![
        (
            "message_delta",
            serde_json::to_string(&json!({
                "type": "message_delta",
                "delta": {"stop_reason": "tool_use", "stop_sequence": null},
                "usage": {"output_tokens": 24}
            }))?,
        ),
        (
            "message_stop",
            serde_json::to_string(&json!({"type": "message_stop"}))?,
        ),
    ])
}

fn wire_id(message_id: &str, tool: &str) -> String {
    format!("toolu_{tool}_{message_id}")
}

fn three_tool_use_sse_events(message_id: &str) -> anyhow::Result<Vec<(&'static str, String)>> {
    let bash_input = json!({"command": "ls -la", "description": "list files"});
    let read_input = json!({"file_path": "/tmp/sample.txt"});
    let agent_input = json!({
        "subagent_type": "researcher-c6",
        "description": "C6 settle smoke",
        "prompt": "investigate"
    });

    let mut events = vec![message_start_event(message_id)?];
    events.extend(tool_use_block_events(
        0,
        &wire_id(message_id, "bash"),
        "Bash",
        &bash_input,
    )?);
    events.extend(tool_use_block_events(
        1,
        &wire_id(message_id, "read"),
        "Read",
        &read_input,
    )?);
    events.extend(tool_use_block_events(
        2,
        &wire_id(message_id, "agent"),
        "Agent",
        &agent_input,
    )?);
    events.extend(terminal_tool_use_events()?);
    Ok(events)
}

async fn drive_streaming_request(proxy_url: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header(
            "x-claude-code-session-id",
            "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        )
        .json(
            &RequestBody::default()
                .stream(true)
                .content("drive C6")
                .build(),
        )
        .send()
        .await?;
    anyhow::ensure!(resp.status() == 200, "expected 200, got {}", resp.status());
    let _ = resp.bytes().await?;
    Ok(())
}

async fn wait_for_request_row(
    pool: &PgPool,
    upstream_message_id: &str,
) -> anyhow::Result<(i64, chrono::DateTime<chrono::Utc>)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let row: Option<(i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT r.id, r.created_at
               FROM requests.requests r
               JOIN responses.responses rr
                 ON rr.request_id = r.id
              WHERE rr.upstream_message_id = $1
              ORDER BY r.created_at DESC
              LIMIT 1",
        )
        .bind(upstream_message_id)
        .fetch_optional(pool)
        .await?;
        if let Some(rc) = row {
            return Ok(rc);
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for requests row for upstream_message_id={upstream_message_id}"
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_tool_uses(
    pool: &PgPool,
    request_id: i64,
    expected: usize,
) -> anyhow::Result<Vec<ToolUseRow>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let rows = sqlx::query(
            "SELECT wire_tool_use_id AS tool_id, ord, tool_name, input,
                    file_path, operation
               FROM tools.uses
              WHERE request_id = $1
              ORDER BY ord ASC",
        )
        .bind(request_id)
        .fetch_all(pool)
        .await?;
        if rows.len() == expected {
            return Ok(rows
                .into_iter()
                .map(|r| ToolUseRow {
                    tool_id: r.get::<String, _>("tool_id"),
                    ord: r.get::<i32, _>("ord"),
                    tool_name: r.get::<String, _>("tool_name"),
                    input: r.get::<Option<Value>, _>("input"),
                    file_path: r.get::<Option<String>, _>("file_path"),
                    operation: r.get::<Option<String>, _>("operation"),
                })
                .collect());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for {expected} tool_uses rows for request_id={request_id}; saw {}",
                rows.len()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[derive(Debug)]
struct ToolUseRow {
    tool_id: String,
    ord: i32,
    tool_name: String,
    input: Option<Value>,
    file_path: Option<String>,
    operation: Option<String>,
}

#[tokio::test]
async fn tool_uses_persisted_with_input_full_fidelity() -> anyhow::Result<()> {
    let message_id = "msg_c6_three_tool_uses_fidelity";
    let events = three_tool_use_sse_events(message_id)?;
    let event_refs: Vec<(&str, &str)> = events.iter().map(|(e, d)| (*e, d.as_str())).collect();
    let raw_sse = sse_body(&event_refs);
    let mock_server = upstream::sse_upstream(&raw_sse).await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping write_path_tool_uses");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    drive_streaming_request(&proxy_url).await?;

    let (request_id, _created_at) = wait_for_request_row(&pool, message_id).await?;
    let rows = wait_for_tool_uses(&pool, request_id, 3).await?;

    assert_eq!(rows[0].ord, 0);
    assert_eq!(rows[0].tool_id, wire_id(message_id, "bash"));
    assert_eq!(rows[0].tool_name, "Bash");
    assert_eq!(rows[1].ord, 1);
    assert_eq!(rows[1].tool_id, wire_id(message_id, "read"));
    assert_eq!(rows[1].tool_name, "Read");
    assert_eq!(rows[2].ord, 2);
    assert_eq!(rows[2].tool_id, wire_id(message_id, "agent"));
    assert_eq!(rows[2].tool_name, "Agent");

    let bash_input_db = rows[0]
        .input
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Bash input must not be NULL"))?;
    assert_eq!(
        bash_input_db,
        &json!({"command": "ls -la", "description": "list files"}),
        "Bash input must round-trip byte-equal through the settle path"
    );
    let read_input_db = rows[1]
        .input
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Read input must not be NULL"))?;
    assert_eq!(
        read_input_db,
        &json!({"file_path": "/tmp/sample.txt"}),
        "Read input must round-trip byte-equal through the settle path"
    );
    let agent_input_db = rows[2]
        .input
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Agent input must not be NULL"))?;
    assert_eq!(
        agent_input_db,
        &json!({
            "subagent_type": "researcher-c6",
            "description": "C6 settle smoke",
            "prompt": "investigate"
        }),
        "Agent input must round-trip byte-equal through the settle path"
    );

    assert!(
        rows[0].file_path.is_none(),
        "Bash: file_path must be NULL (no file_path key in input)"
    );
    assert_eq!(
        rows[0].operation.as_deref(),
        Some("exec"),
        "Bash: operation must be 'exec'"
    );
    assert_eq!(
        rows[1].file_path.as_deref(),
        Some("/tmp/sample.txt"),
        "Read: file_path must be hoisted from input JSON"
    );
    assert_eq!(
        rows[1].operation.as_deref(),
        Some("read"),
        "Read: operation must be 'read'"
    );
    assert!(
        rows[2].file_path.is_none(),
        "Agent: file_path must be NULL (no file_path key in input)"
    );
    assert_eq!(
        rows[2].operation.as_deref(),
        Some("dispatch"),
        "Agent: operation must be 'dispatch'"
    );

    shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn agent_dispatches_resolves_to_agent_via_fk() -> anyhow::Result<()> {
    let message_id = "msg_c6_three_tool_uses_agent_fk";
    let events = three_tool_use_sse_events(message_id)?;
    let event_refs: Vec<(&str, &str)> = events.iter().map(|(e, d)| (*e, d.as_str())).collect();
    let raw_sse = sse_body(&event_refs);
    let mock_server = upstream::sse_upstream(&raw_sse).await;

    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&mock_server.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping write_path_tool_uses");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    drive_streaming_request(&proxy_url).await?;

    let (request_id, _created_at) = wait_for_request_row(&pool, message_id).await?;
    wait_for_tool_uses(&pool, request_id, 3).await?;

    let dispatch: Option<(String, String, String)> = sqlx::query_as(
        "SELECT tu.wire_tool_use_id, a.name, s.code
           FROM agents.dispatches ad
           JOIN agents.agents a ON a.id = ad.agent_id
           JOIN agents.sources s ON s.id = a.source_id
           JOIN tools.uses tu ON tu.id = ad.tool_use_id
          WHERE tu.wire_tool_use_id = $1",
    )
    .bind(wire_id(message_id, "agent"))
    .fetch_optional(&pool)
    .await?;
    let (tool_use_id, agent_name, source) =
        dispatch.ok_or_else(|| anyhow::anyhow!("expected one agent_dispatches row"))?;
    assert_eq!(tool_use_id, wire_id(message_id, "agent"));
    assert_eq!(
        agent_name, "researcher-c6",
        "agents.name must equal the Agent input's subagent_type"
    );
    assert_eq!(
        source, "observed",
        "C6 UPSERT path uses source='observed' (matches existing migration semantics)"
    );

    let stray: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agents.dispatches
          WHERE tool_use_id IN (
              SELECT id FROM tools.uses
               WHERE wire_tool_use_id IN ($1, $2))",
    )
    .bind(wire_id(message_id, "bash"))
    .bind(wire_id(message_id, "read"))
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stray, 0,
        "agent_dispatches must only be created for tool_uses where tool_name='Agent'"
    );

    shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(100)).await;
    test_db.cleanup().await?;
    Ok(())
}
