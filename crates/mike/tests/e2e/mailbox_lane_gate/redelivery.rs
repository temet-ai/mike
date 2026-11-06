use crate::test_db;

use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::matchers::{method, path};

use test_db::TestDb;

use super::support::{
    ack_mailbox_message, boot_proxy, last_block_text, mailbox_redelivery_state, mailbox_row_state,
    ok_response, primary_body, queue_mailbox_message, send_request, wait_for_session_row,
};

struct RedeliveryCheck<'a> {
    client: &'a reqwest::Client,
    proxy_url: &'a str,
    session_str: &'a str,
    inspect_url: &'a str,
    mock: &'a MockServer,
    pool: &'a PgPool,
    session: uuid::Uuid,
}

async fn assert_second_turn_is_redelivery(
    ctx: &RedeliveryCheck<'_>,
    message_uuid: uuid::Uuid,
    distinctive_text: &str,
) -> anyhow::Result<()> {
    send_request(
        ctx.client,
        ctx.proxy_url,
        ctx.session_str,
        &primary_body("second turn"),
    )
    .await?;
    let received = ctx
        .mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 3, "bootstrap + first delivery + redelivery");
    let redelivery: Value = serde_json::from_slice(&received[2].body)?;
    assert!(
        last_block_text(&redelivery)?.contains(distinctive_text),
        "second primary turn must carry the SAME mailbox envelope again (redelivery)"
    );
    let (attempts, consumed, dead_lettered) =
        mailbox_redelivery_state(ctx.pool, message_uuid).await?;
    assert_eq!(attempts, 2, "redelivery increments delivery_attempts to 2");
    assert!(!consumed, "still not consumed before the ack");
    assert!(!dead_lettered, "not dead-lettered — cap default is 5");
    Ok(())
}

async fn assert_ack_then_no_further_delivery(
    ctx: &RedeliveryCheck<'_>,
    message_uuid: uuid::Uuid,
    distinctive_text: &str,
) -> anyhow::Result<()> {
    let ack_status =
        ack_mailbox_message(ctx.client, ctx.inspect_url, ctx.session, message_uuid).await?;
    assert_eq!(ack_status, reqwest::StatusCode::OK, "ack must succeed");
    let (_attempts, consumed, _dead_lettered) =
        mailbox_redelivery_state(ctx.pool, message_uuid).await?;
    assert!(consumed, "consumed_at must be set after the ack");

    send_request(
        ctx.client,
        ctx.proxy_url,
        ctx.session_str,
        &primary_body("third turn"),
    )
    .await?;
    let received = ctx
        .mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(
        received.len(),
        4,
        "bootstrap + first delivery + redelivery + post-ack"
    );
    let post_ack: Value = serde_json::from_slice(&received[3].body)?;
    assert!(
        !last_block_text(&post_ack)?.contains(distinctive_text),
        "post-ack primary turn must NOT carry the mailbox envelope again"
    );
    Ok(())
}

#[tokio::test]
async fn mailbox_redelivered_on_second_primary_turn_then_stops_after_ack() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping mailbox_redelivered_on_second_primary_turn_then_stops_after_ack"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    sqlx::query(
        "INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES ('mailbox_redelivery', true, true)
         ON CONFLICT (key) DO UPDATE SET enabled = true, operator_set = true",
    )
    .execute(&pool)
    .await?;

    let (_state, proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_str = session.to_string();

    send_request(&client, &proxy_url, &session_str, &primary_body("init")).await?;
    wait_for_session_row(&pool, session).await?;

    let distinctive_text = "redelivery-pin: isiZulu must be one of the 12 languages";
    let message_uuid =
        queue_mailbox_message(&client, &inspect_url, session, distinctive_text).await?;

    send_request(
        &client,
        &proxy_url,
        &session_str,
        &primary_body("first turn"),
    )
    .await?;
    let (claimed, delivered) = mailbox_row_state(&pool, message_uuid).await?;
    assert!(claimed, "row must be claimed after the first delivery");
    assert!(delivered, "row must be sent after the first delivery");
    let (attempts, consumed, dead_lettered) = mailbox_redelivery_state(&pool, message_uuid).await?;
    assert_eq!(attempts, 1, "first send counts as delivery_attempts == 1");
    assert!(!consumed, "not consumed after the first delivery");
    assert!(!dead_lettered, "not dead-lettered after one attempt");

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    let first_delivery: Value = serde_json::from_slice(&received[1].body)?;
    assert!(
        last_block_text(&first_delivery)?.contains(distinctive_text),
        "first primary turn must carry the mailbox envelope"
    );

    let ctx = RedeliveryCheck {
        client: &client,
        proxy_url: &proxy_url,
        session_str: &session_str,
        inspect_url: &inspect_url,
        mock: &mock,
        pool: &pool,
        session,
    };

    assert_second_turn_is_redelivery(&ctx, message_uuid, distinctive_text).await?;
    assert_ack_then_no_further_delivery(&ctx, message_uuid, distinctive_text).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
