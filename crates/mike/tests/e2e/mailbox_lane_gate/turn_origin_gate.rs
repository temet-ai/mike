use crate::test_db;

use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use test_db::TestDb;

use super::support::{
    boot_proxy, last_block_text, mailbox_row_state, ok_response, primary_body,
    queue_mailbox_message, send_request, subagent_body, wait_for_session_row,
    workflow_subagent_tool_loop_body,
};

struct GateCheck<'a> {
    client: &'a reqwest::Client,
    proxy_url: &'a str,
    session_str: &'a str,
    mock: &'a MockServer,
    pool: &'a PgPool,
}

async fn assert_subagent_turn_skips_mailbox_injection(
    ctx: &GateCheck<'_>,
    message_uuid: uuid::Uuid,
    expected_received: usize,
    distinctive_text: &str,
) -> anyhow::Result<()> {
    send_request(
        ctx.client,
        ctx.proxy_url,
        ctx.session_str,
        &subagent_body("please review this function"),
    )
    .await?;

    let received = ctx
        .mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(
        received.len(),
        expected_received,
        "expected {expected_received} forwarded requests so far (bootstrap + subagent)"
    );
    let subagent_forwarded: Value = serde_json::from_slice(&received[expected_received - 1].body)?;
    let subagent_last_block = last_block_text(&subagent_forwarded)?;
    assert!(
        !subagent_last_block.contains("<system-reminder>"),
        "subagent turn must NOT receive a system-reminder block; got: {subagent_last_block}"
    );
    assert!(
        !subagent_last_block.contains(distinctive_text),
        "subagent turn must NOT receive the mailbox message text; got: {subagent_last_block}"
    );

    let (claimed, delivered) = mailbox_row_state(ctx.pool, message_uuid).await?;
    assert!(
        !claimed,
        "mailbox row must remain unclaimed after a subagent turn"
    );
    assert!(
        !delivered,
        "mailbox row must remain undelivered after a subagent turn"
    );
    Ok(())
}

async fn assert_primary_turn_delivers_mailbox_envelope(
    ctx: &GateCheck<'_>,
    message_uuid: uuid::Uuid,
    expected_received: usize,
    distinctive_text: &str,
) -> anyhow::Result<()> {
    send_request(
        ctx.client,
        ctx.proxy_url,
        ctx.session_str,
        &primary_body("continue the task"),
    )
    .await?;

    let received = ctx
        .mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(
        received.len(),
        expected_received,
        "expected {expected_received} forwarded requests total (bootstrap + subagent + primary)"
    );
    let primary_forwarded: Value = serde_json::from_slice(&received[expected_received - 1].body)?;
    let primary_last_block = last_block_text(&primary_forwarded)?;
    assert!(
        primary_last_block.contains("<system-reminder>"),
        "primary turn must receive the mailbox system-reminder block; got: {primary_last_block}"
    );
    assert!(
        primary_last_block.contains(distinctive_text),
        "primary turn must receive the full mailbox message text (no truncation); got: {primary_last_block}"
    );

    let (claimed, _delivered) = mailbox_row_state(ctx.pool, message_uuid).await?;
    assert!(
        claimed,
        "mailbox row must be claimed after delivery on a primary turn"
    );
    Ok(())
}

#[tokio::test]
async fn mailbox_reminder_skipped_on_subagent_turn_then_delivered_on_primary_turn()
-> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping mailbox_reminder_skipped_on_subagent_turn_then_delivered_on_primary_turn"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (_state, proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_str = session.to_string();

    send_request(&client, &proxy_url, &session_str, &primary_body("init")).await?;
    wait_for_session_row(&pool, session).await?;

    let distinctive_text = "gate-pin: rerun the flaky suite with --retries 2";
    let message_uuid =
        queue_mailbox_message(&client, &inspect_url, session, distinctive_text).await?;

    let (claimed, delivered) = mailbox_row_state(&pool, message_uuid).await?;
    assert!(!claimed, "row must be pending immediately after queueing");
    assert!(
        !delivered,
        "row must not be delivered immediately after queueing"
    );

    let ctx = GateCheck {
        client: &client,
        proxy_url: &proxy_url,
        session_str: &session_str,
        mock: &mock,
        pool: &pool,
    };

    assert_subagent_turn_skips_mailbox_injection(&ctx, message_uuid, 2, distinctive_text).await?;
    assert_primary_turn_delivers_mailbox_envelope(&ctx, message_uuid, 3, distinctive_text).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mailbox_reminder_skipped_on_workflow_subagent_tool_loop_turn() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping mailbox_reminder_skipped_on_workflow_subagent_tool_loop_turn"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (_state, proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_str = session.to_string();

    send_request(&client, &proxy_url, &session_str, &primary_body("init")).await?;
    wait_for_session_row(&pool, session).await?;

    let distinctive_text = "gate-pin: coordinator ruling on the verification approach";
    let message_uuid =
        queue_mailbox_message(&client, &inspect_url, session, distinctive_text).await?;

    let (claimed, delivered) = mailbox_row_state(&pool, message_uuid).await?;
    assert!(!claimed, "row must be pending immediately after queueing");
    assert!(
        !delivered,
        "row must not be delivered immediately after queueing"
    );

    send_request(
        &client,
        &proxy_url,
        &session_str,
        &workflow_subagent_tool_loop_body("verify the fix"),
    )
    .await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(
        received.len(),
        2,
        "expected 2 forwarded requests so far (bootstrap + workflow-subagent tool_loop)"
    );
    let forwarded_text = String::from_utf8_lossy(&received[1].body).into_owned();
    assert!(
        !forwarded_text.contains("<system-reminder>"),
        "workflow-subagent tool_loop turn must NOT receive a system-reminder block; got: {forwarded_text}"
    );
    assert!(
        !forwarded_text.contains(distinctive_text),
        "workflow-subagent tool_loop turn must NOT receive the mailbox message text; got: {forwarded_text}"
    );

    let (claimed, delivered) = mailbox_row_state(&pool, message_uuid).await?;
    assert!(
        !claimed,
        "mailbox row must remain unclaimed after a workflow-subagent tool_loop turn"
    );
    assert!(
        !delivered,
        "mailbox row must remain undelivered after a workflow-subagent tool_loop turn"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
