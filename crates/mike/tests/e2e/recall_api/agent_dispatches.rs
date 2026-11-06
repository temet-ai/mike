use crate::db_helpers;

use serde_json::{Value, json};

use super::support::{InsertRequestParams, bootstrap_project_session, insert_request};

#[tokio::test]
async fn agent_dispatches_returns_full_description_and_statistical_brief_len() -> anyhow::Result<()>
{
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, pool, test_db) = proxy.into_parts();

    let project_path = "test-agent-dispatches";
    let session_id = "12345678-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let project_id = bootstrap_project_session(&pool, project_path, session_id).await?;

    let r = insert_request(
        &pool,
        InsertRequestParams {
            session_id,
            project_id,
            turn_origin: "primary",
            turn_type: "user_prompt",
            human_prompt: "spawn rust-developer",
            created_at_offset_ms: 0,
        },
    )
    .await?;

    let (r_ca,): (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT created_at FROM requests.requests WHERE id = $1")
            .bind(r)
            .fetch_one(&pool)
            .await?;
    let agent_id: uuid::Uuid = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO agents.agents (name)
             SELECT 'rust-developer'
             WHERE NOT EXISTS (
                 SELECT 1 FROM agents.agents WHERE name = 'rust-developer'
             )
             RETURNING uuid
         )
         SELECT uuid FROM ins
         UNION ALL
         SELECT uuid FROM agents.agents WHERE name = 'rust-developer'
         LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    let wire_tool_id = format!("toolu_agentdispatch_{r}");
    sqlx::query(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input, created_at, session_id)
         VALUES ($1, $2, 0, 'Agent', $3, $4,
                 (SELECT session_id FROM requests.requests WHERE id = $2))",
    )
    .bind(&wire_tool_id)
    .bind(r)
    .bind(json!({
        "subagent_type": "rust-developer",
        "description": "phase-4 agent dispatches",
        "prompt": "x".repeat(5_000),
    }))
    .bind(r_ca)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO agents.dispatches (tool_use_id, agent_id) VALUES (
            (SELECT id FROM tools.uses WHERE wire_tool_use_id = $1),
            (SELECT id FROM agents.agents WHERE uuid = $2))",
    )
    .bind(&wire_tool_id)
    .bind(agent_id)
    .execute(&pool)
    .await?;

    let client = reqwest::Client::new();

    let url = format!(
        "{}/api/agent-dispatches?project={}",
        inspect_url, project_path
    );
    let resp = client.get(&url).send().await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let rows: Vec<Value> = resp.json().await?;
    assert_eq!(rows.len(), 1, "expected one dispatch row, got {rows:?}");
    let row = &rows[0];
    assert_eq!(row["description"], "phase-4 agent dispatches");
    assert_eq!(row["subagent_type"], "rust-developer");
    assert_eq!(row["brief_char_len"], 5_000);
    assert!(row["brief_sha256"].is_string());
    assert!(
        row.get("prompt").is_none(),
        "prompt must not be inlined into agent_dispatches: {row}"
    );

    test_db.cleanup().await?;
    Ok(())
}
