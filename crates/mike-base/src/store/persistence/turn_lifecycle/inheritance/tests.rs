use super::*;
use crate::store::models::{InterceptedRequest, RequestId, RequestStatus};
use crate::store::persistence::{PersistHandle, request_write::IdMode, test_support};

async fn request(
    handle: &PersistHandle,
    session: &str,
    id: u64,
    origin: &str,
) -> anyhow::Result<InterceptedRequest> {
    let mut req = crate::store::tests::make_test_request(Some(RequestId::from_raw(id)), session);
    req.turn_origin = origin.into();
    req.turn_type = "user_prompt".into();
    req.human_prompt = "Implement the requested change".into();
    handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    Ok(req)
}

async fn complete(handle: &PersistHandle, req: &mut InterceptedRequest) -> anyhow::Result<()> {
    req.status = RequestStatus::Complete;
    req.completed_at = Some(chrono::Utc::now());
    handle.snapshot_request(req).await?;
    Ok(())
}

async fn turn(handle: &PersistHandle, id: u64) -> anyhow::Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT operator_turn_id FROM requests.requests WHERE id = $1")
            .bind(id as i64)
            .fetch_one(&handle.pool)
            .await?,
    )
}

async fn dispatch(handle: &PersistHandle, parent: u64, child: u64) -> anyhow::Result<()> {
    sqlx::query(
        "WITH agent AS (
            INSERT INTO agents.agents (name) VALUES ($3) RETURNING id
        ), tool AS (
            INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, session_id)
            SELECT $3, id, 0, 'Agent', session_id FROM requests.requests WHERE id = $1 RETURNING id
        ), dispatch AS (
            INSERT INTO agents.dispatches (tool_use_id, agent_id)
            SELECT tool.id, agent.id FROM tool, agent RETURNING id, agent_id
        ), instance AS (
            INSERT INTO agents.instances (session_id, dispatch_id, agent_id, first_request_id)
            SELECT r.session_id, d.id, d.agent_id, r.id
            FROM dispatch d, requests.requests r WHERE r.id = $2 RETURNING id
        ) UPDATE requests.requests SET agent_instance_id = instance.id
          FROM instance WHERE requests.requests.id = $2",
    )
    .bind(parent as i64)
    .bind(child as i64)
    .bind(format!("dispatch-{parent}-{child}"))
    .execute(&handle.pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn child_session_never_takes_the_parent_sessions_turn_and_abort_preserves_parent_lifecycle()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let parent_session = uuid::Uuid::new_v4().to_string();
    let child_session = uuid::Uuid::new_v4().to_string();
    let mut parent = request(&handle, &parent_session, 910001, "primary").await?;
    complete(&handle, &mut parent).await?;
    let parent_turn = turn(&handle, 910001).await?;
    assert!(parent_turn.is_some());
    let mut next = request(&handle, &parent_session, 910002, "primary").await?;
    complete(&handle, &mut next).await?;
    let mut child = request(&handle, &child_session, 910003, "subagent").await?;
    assert_eq!(turn(&handle, 910003).await?, None);
    dispatch(&handle, 910001, 910003).await?;
    complete(&handle, &mut child).await?;
    assert_eq!(turn(&handle, 910003).await?, None);
    let _aborted = request(&handle, &child_session, 910004, "subagent").await?;
    assert_eq!(turn(&handle, 910004).await?, None);
    handle.close_aborted_request(910004).await?;
    assert_eq!(turn(&handle, 910004).await?, None);
    assert!(turn(&handle, 910001).await?.is_some());
    let open: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.operator_turns WHERE closed_at IS NULL")
            .fetch_one(&handle.pool)
            .await?;
    assert_eq!(open, 1);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn inline_worker_retains_arrival_turn_when_parent_advances() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let mut parent = request(&handle, &session, 920001, "primary").await?;
    complete(&handle, &mut parent).await?;
    let first_turn = turn(&handle, 920001).await?;
    let mut worker = request(&handle, &session, 920002, "subagent").await?;
    assert_eq!(turn(&handle, 920002).await?, first_turn);
    let mut next = request(&handle, &session, 920003, "primary").await?;
    complete(&handle, &mut next).await?;
    complete(&handle, &mut worker).await?;
    assert_eq!(turn(&handle, 920002).await?, first_turn);
    sqlx::query("UPDATE requests.requests SET operator_turn_id = NULL WHERE id = 920002")
        .execute(&handle.pool)
        .await?;
    let mut tx = handle.pool.begin().await?;
    attribute_inherited_requests(&mut tx, 920002).await?;
    attribute_inherited_requests(&mut tx, 920002).await?;
    tx.commit().await?;
    assert_eq!(turn(&handle, 920002).await?, first_turn);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_spawned_helper_session_keeps_its_requests_out_of_the_spawning_turn() -> anyhow::Result<()>
{
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let parent_session = uuid::Uuid::new_v4().to_string();
    let child_session = uuid::Uuid::new_v4().to_string();
    let mut parent = request(&handle, &parent_session, 930001, "primary").await?;
    complete(&handle, &mut parent).await?;
    let parent_turn = turn(&handle, 930001).await?;
    let mut child = request(&handle, &child_session, 930002, "subagent").await?;
    child.response_status = Some(200);
    child.token_usage = Some(crate::types::TokenUsage {
        input_tokens: 123,
        output_tokens: 45,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
    });
    complete(&handle, &mut child).await?;
    assert_eq!(turn(&handle, 930002).await?, None);
    let content = serde_json::json!([{"type":"text", "text":
        serde_json::json!({"launch":{"session":child_session}}).to_string()}]);
    sqlx::query(
        "WITH tool AS (
            INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, session_id)
            SELECT 'spawn-test', id, 0, 'mcp__mike__spawn_session', session_id
            FROM requests.requests WHERE id=930001 RETURNING id, session_id
        ) INSERT INTO tools.results (tool_use_id, request_id, ord, is_error, content, content_sha256, content_bytes, session_id)
          SELECT id, 930001, 0, false, $1, 'test-sha', 1, session_id FROM tool",
    )
    .bind(serde_json::json!([{"type":"text","text":"not JSON"}]))
    .execute(&handle.pool)
    .await?;
    let mut invalid = handle.pool.begin().await?;
    reconcile_dispatch_results(&mut invalid, &["spawn-test".into()]).await?;
    invalid.commit().await?;
    assert_eq!(turn(&handle, 930002).await?, None);
    sqlx::query("UPDATE tools.results SET content=$1 WHERE request_id=930001")
        .bind(content)
        .execute(&handle.pool)
        .await?;
    let mut tx = handle.pool.begin().await?;
    reconcile_dispatch_results(&mut tx, &["spawn-test".into()]).await?;
    tx.commit().await?;
    assert_eq!(turn(&handle, 930002).await?, None);
    let tokens: (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT total_input_tokens, total_output_tokens FROM memory.operator_turns WHERE id=$1",
    )
    .bind(parent_turn)
    .fetch_one(&handle.pool)
    .await?;
    assert_ne!(tokens, (Some(123), Some(45)));
    let mut followup = request(&handle, &child_session, 930003, "primary").await?;
    complete(&handle, &mut followup).await?;
    let own_turn = turn(&handle, 930003).await?;
    assert!(own_turn.is_some());
    assert_ne!(own_turn, parent_turn);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn missing_request_is_noop_and_database_errors_propagate() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut tx = handle.pool.begin().await?;
    attribute_inherited_requests(&mut tx, i64::MAX).await?;
    assert_eq!(inherited_request_turn(&mut tx, i64::MAX).await?, None);
    sqlx::query("DROP VIEW memory.request_dispatch_parents CASCADE")
        .execute(&mut *tx)
        .await?;
    assert!(
        attribute_inherited_requests(&mut tx, i64::MAX)
            .await
            .is_err()
    );
    tx.rollback().await?;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_later_session_spawned_down_a_chain_gets_no_turn_and_cycles_remain_unknown()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let sessions: Vec<String> = (0..5).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    let mut root = request(&handle, &sessions[0], 940001, "primary").await?;
    complete(&handle, &mut root).await?;
    for (offset, session) in sessions.iter().enumerate().skip(1) {
        request(&handle, session, 940001 + offset as u64, "subagent").await?;
    }
    dispatch(&handle, 940001, 940002).await?;
    dispatch(&handle, 940002, 940003).await?;
    dispatch(&handle, 940004, 940005).await?;
    dispatch(&handle, 940005, 940004).await?;
    let mut tx = handle.pool.begin().await?;
    attribute_inherited_requests(&mut tx, 940003).await?;
    attribute_inherited_requests(&mut tx, 940004).await?;
    tx.commit().await?;
    assert!(turn(&handle, 940001).await?.is_some());
    assert_eq!(turn(&handle, 940003).await?, None);
    assert_eq!(turn(&handle, 940004).await?, None);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn producer_link_selects_spawning_turn_instead_of_newer_open_turn() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let mut root = request(&handle, &session, 950001, "primary").await?;
    complete(&handle, &mut root).await?;
    let mut next = request(&handle, &session, 950002, "primary").await?;
    complete(&handle, &mut next).await?;
    request(&handle, &session, 950003, "subagent").await?;
    dispatch(&handle, 950001, 950003).await?;
    sqlx::query("UPDATE agents.dispatches SET worker_producer_id='worker-test'")
        .execute(&handle.pool)
        .await?;
    sqlx::query(
        "UPDATE requests.requests SET operator_turn_id=NULL, agent_instance_id=NULL,
        request_headers='[[\"x-claude-code-agent-id\",\"worker-test\"]]'::jsonb WHERE id=950003",
    )
    .execute(&handle.pool)
    .await?;
    let mut tx = handle.pool.begin().await?;
    attribute_inherited_requests(&mut tx, 950003).await?;
    tx.commit().await?;
    assert_eq!(turn(&handle, 950003).await?, turn(&handle, 950001).await?);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn backfill_command_repairs_history_and_advances_past_unknown_requests() -> anyhow::Result<()>
{
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4().to_string();
    let mut root = request(&handle, &session, 960001, "primary").await?;
    complete(&handle, &mut root).await?;
    request(&handle, &session, 960002, "subagent").await?;
    request(
        &handle,
        &uuid::Uuid::new_v4().to_string(),
        960003,
        "subagent",
    )
    .await?;
    sqlx::query("UPDATE requests.requests SET operator_turn_id=NULL WHERE id=960002")
        .execute(&handle.pool)
        .await?;
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/backfill-operator-turns");
    for _ in 0..2 {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::process::Command::new(&script)
                .env("MIKE_DATABASE_URL", &db.database_url)
                .output(),
        )
        .await??;
        anyhow::ensure!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(turn(&handle, 960002).await?, turn(&handle, 960001).await?);
        assert_eq!(turn(&handle, 960003).await?, None);
    }
    db.cleanup().await?;
    Ok(())
}
