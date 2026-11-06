use mike::store::persistence::assemble::assemble_message_json;

use super::support::{
    SeedBlock, seed_block, seed_project, seed_session, seed_streaming_request, seed_tool_use,
};
use crate::test_db::TestDb;

#[tokio::test]
async fn assemble_interleaves_text_tool_use_text_in_order() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id =
        seed_project(&pool, &format!("assemble-proj-{}", uuid::Uuid::new_v4())).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let req_id = seed_streaming_request(&pool, session_fk, project_id, "claude-sonnet-4").await?;

    seed_block(
        &pool,
        SeedBlock {
            request_id: req_id,
            session_id_fk: session_fk,
            seq_ord: 0,
            content: "before the tool",
            block_code: "text",
            signature: None,
            wire_tool_use_id: None,
        },
    )
    .await?;
    seed_block(
        &pool,
        SeedBlock {
            request_id: req_id,
            session_id_fk: session_fk,
            seq_ord: 1,
            content: "",
            block_code: "tool_use",
            signature: None,
            wire_tool_use_id: Some("toolu_abc"),
        },
    )
    .await?;
    seed_block(
        &pool,
        SeedBlock {
            request_id: req_id,
            session_id_fk: session_fk,
            seq_ord: 2,
            content: "after the tool",
            block_code: "text",
            signature: None,
            wire_tool_use_id: None,
        },
    )
    .await?;

    let tool_input = serde_json::json!({"path": "/tmp/x"});
    seed_tool_use(&pool, req_id, "toolu_abc", 0, "Read", &tool_input).await?;

    let assembled = assemble_message_json(&pool, req_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected Some(assembled message)"))?;

    assert_eq!(assembled["type"].as_str(), Some("message"));
    assert_eq!(assembled["role"].as_str(), Some("assistant"));
    assert_eq!(assembled["model"].as_str(), Some("claude-sonnet-4"));

    let content = assembled["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be an array"))?;
    assert_eq!(content.len(), 3, "three blocks: text, tool_use, text");

    assert_eq!(content[0]["type"].as_str(), Some("text"));
    assert_eq!(content[0]["text"].as_str(), Some("before the tool"));

    assert_eq!(content[1]["type"].as_str(), Some("tool_use"));
    assert_eq!(content[1]["id"].as_str(), Some("toolu_abc"));
    assert_eq!(content[1]["name"].as_str(), Some("Read"));
    assert_eq!(content[1]["input"]["path"].as_str(), Some("/tmp/x"));

    assert_eq!(content[2]["type"].as_str(), Some("text"));
    assert_eq!(content[2]["text"].as_str(), Some("after the tool"));

    Ok(())
}

#[tokio::test]
async fn assemble_emits_thinking_signature_when_present() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("assemble-sig-proj-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let req_id = seed_streaming_request(&pool, session_fk, project_id, "claude-opus-4").await?;

    seed_block(
        &pool,
        SeedBlock {
            request_id: req_id,
            session_id_fk: session_fk,
            seq_ord: 0,
            content: "let me reason",
            block_code: "thinking",
            signature: Some("sig-xyz=="),
            wire_tool_use_id: None,
        },
    )
    .await?;
    seed_block(
        &pool,
        SeedBlock {
            request_id: req_id,
            session_id_fk: session_fk,
            seq_ord: 1,
            content: "more reasoning",
            block_code: "thinking",
            signature: None,
            wire_tool_use_id: None,
        },
    )
    .await?;

    let assembled = assemble_message_json(&pool, req_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected Some(assembled message)"))?;
    let content = assembled["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be an array"))?;
    assert_eq!(content.len(), 2);

    assert_eq!(content[0]["type"].as_str(), Some("thinking"));
    assert_eq!(content[0]["thinking"].as_str(), Some("let me reason"));
    assert_eq!(
        content[0]["signature"].as_str(),
        Some("sig-xyz=="),
        "thinking block must carry the captured signature"
    );

    assert_eq!(content[1]["type"].as_str(), Some("thinking"));
    assert!(
        content[1].get("signature").is_none(),
        "thinking block without a captured signature must omit the field"
    );

    Ok(())
}

#[tokio::test]
async fn assemble_returns_none_without_blocks() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("assemble-empty-proj-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let req_id = seed_streaming_request(&pool, session_fk, project_id, "claude-sonnet-4").await?;

    let assembled = assemble_message_json(&pool, req_id).await?;
    assert!(assembled.is_none(), "no blocks → None");

    Ok(())
}

#[tokio::test]
async fn assemble_carries_stop_sequence_input_transformations_and_wire_usage() -> anyhow::Result<()>
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id =
        seed_project(&pool, &format!("assemble-extras-{}", uuid::Uuid::new_v4())).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let req_id = seed_streaming_request(&pool, session_fk, project_id, "fable-5-1").await?;
    seed_block(
        &pool,
        SeedBlock {
            request_id: req_id,
            session_id_fk: session_fk,
            seq_ord: 0,
            content: "hello",
            block_code: "text",
            signature: None,
            wire_tool_use_id: None,
        },
    )
    .await?;

    let usage = serde_json::json!({
        "input_tokens": 2,
        "output_tokens": 77,
        "cache_creation_input_tokens": 29271,
        "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 29271},
        "service_tier": "standard"
    });
    let extras = serde_json::json!({
        "stop_sequence": "</done>",
        "input_transformations": [{"reason": "prefix_binding_mismatch"}]
    });
    sqlx::query(
        "INSERT INTO responses.responses
            (request_id, response_status, token_usage, message_extras, completed_at)
         SELECT $1, (SELECT id FROM responses.statuses LIMIT 1), $2, $3, now()",
    )
    .bind(req_id)
    .bind(&usage)
    .bind(&extras)
    .execute(&pool)
    .await?;

    let assembled = assemble_message_json(&pool, req_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected Some(assembled message)"))?;

    assert_eq!(assembled["stop_sequence"], "</done>");
    assert_eq!(
        assembled["input_transformations"][0]["reason"],
        "prefix_binding_mismatch"
    );
    assert_eq!(assembled["usage"], usage);
    Ok(())
}
