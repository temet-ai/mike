use super::RECONCILE_CALLS;
use crate::store::models::{RequestId, ToolResult};
use crate::store::persistence::{PersistHandle, request_write::IdMode, test_support};

fn calls() -> usize {
    RECONCILE_CALLS.with(|calls| calls.get())
}

fn tool_result(id: &str, text: &str) -> ToolResult {
    ToolResult {
        tool_use_id: id.to_string(),
        is_error: false,
        content_sha256: "test-sha".into(),
        content_bytes: text.len() as u64,
        content: serde_json::json!([{"type": "text", "text": text}]),
    }
}

async fn seed_tool_use(
    handle: &PersistHandle,
    id: u64,
    wire_id: &str,
    name: &str,
) -> anyhow::Result<()> {
    let session = uuid::Uuid::new_v4().to_string();
    let mut req = crate::store::tests::make_test_request(Some(RequestId::from_raw(id)), &session);
    req.tool_calls = vec![crate::types::ToolCall {
        tool_id: wire_id.to_string(),
        name: name.to_string(),
        input_json: "{}".to_string(),
        complete: true,
    }];
    handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    Ok(())
}

const AGENT_TEXT: &str = "Async agent launched.\nagentId: a53fd635aaceeed5c (internal ID)";

async fn write(handle: &PersistHandle, id: u64, results: Vec<ToolResult>) -> anyhow::Result<()> {
    let session = uuid::Uuid::new_v4().to_string();
    let mut req = crate::store::tests::make_test_request(Some(RequestId::from_raw(id)), &session);
    req.tool_results_in_body = results;
    handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    Ok(())
}

#[tokio::test]
async fn request_write_no_dispatch_result_never_reconciles() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let before = calls();
    write(&handle, 940001, Vec::new()).await?;
    write(
        &handle,
        940002,
        vec![
            tool_result("toolu_read", "file contents"),
            tool_result("toolu_bash", "exit 0"),
        ],
    )
    .await?;
    assert_eq!(calls() - before, 0);
    Ok(())
}

#[tokio::test]
async fn request_write_one_agent_result_reconciles_exactly_once() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_tool_use(&handle, 940010, "toolu_agent", "Agent").await?;
    seed_tool_use(&handle, 940011, "toolu_read", "Read").await?;
    let before = calls();
    write(
        &handle,
        940003,
        vec![
            tool_result("toolu_read", "file contents"),
            tool_result("toolu_agent", AGENT_TEXT),
        ],
    )
    .await?;
    assert_eq!(calls() - before, 1);
    Ok(())
}

#[tokio::test]
async fn request_write_history_agent_result_without_new_one_never_reconciles() -> anyhow::Result<()>
{
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_tool_use(&handle, 940020, "toolu_agent", "Agent").await?;
    write(
        &handle,
        940021,
        vec![tool_result("toolu_agent", AGENT_TEXT)],
    )
    .await?;
    seed_tool_use(&handle, 940022, "toolu_read", "Read").await?;
    let before = calls();
    write(
        &handle,
        940023,
        vec![
            tool_result("toolu_agent", AGENT_TEXT),
            tool_result("toolu_read", "file contents"),
        ],
    )
    .await?;
    assert_eq!(calls() - before, 0);
    Ok(())
}

#[tokio::test]
async fn request_write_bash_result_quoting_agent_id_never_reconciles() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_tool_use(&handle, 940030, "toolu_bash", "Bash").await?;
    let before = calls();
    write(
        &handle,
        940031,
        vec![tool_result("toolu_bash", "agentId: x")],
    )
    .await?;
    assert_eq!(calls() - before, 0);
    Ok(())
}
