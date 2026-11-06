use crate::test_db;

use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use test_db::TestDb;

use super::support::{
    backdate_queued_at, boot_proxy, failed_sends_count, last_block_text, mailbox_reap_state,
    mailbox_row_state, ok_response, primary_body, queue_mailbox_message, seed_captured_request,
    seed_silent_session, send_request, wait_for_dead_letter, wait_for_session_id,
};

async fn assert_silent_companion_reaped_with_failed_send(
    pool: &PgPool,
    silent_message_uuid: uuid::Uuid,
    silent_session: uuid::Uuid,
) -> anyhow::Result<()> {
    wait_for_dead_letter(pool, silent_message_uuid).await?;

    let (silent_attempts, silent_dead_lettered) =
        mailbox_reap_state(pool, silent_message_uuid).await?;
    assert_eq!(
        silent_attempts, 0,
        "the silent companion was never delivered before being reaped"
    );
    assert!(
        silent_dead_lettered,
        "the silent companion (no activity in any lane) must be reaped"
    );
    assert_eq!(
        failed_sends_count(pool, &silent_session.to_string(), "strand_ttl_expired").await?,
        1,
        "reaping the silent companion must log exactly one failed_sends row"
    );
    Ok(())
}

async fn assert_busy_recipient_untouched_by_reaper(
    pool: &PgPool,
    busy_message_uuid: uuid::Uuid,
    busy_session: uuid::Uuid,
) -> anyhow::Result<()> {
    let (busy_attempts, busy_dead_lettered) = mailbox_reap_state(pool, busy_message_uuid).await?;
    assert_eq!(
        busy_attempts, 0,
        "the busy recipient's row must still show zero delivery attempts"
    );
    assert!(
        !busy_dead_lettered,
        "a recipient with subagent-lane activity in the window must not be dead-lettered"
    );
    assert_eq!(
        failed_sends_count(pool, &busy_session.to_string(), "strand_ttl_expired").await?,
        0,
        "the busy recipient must not appear in failed_sends at all"
    );
    let (busy_claimed, busy_delivered) = mailbox_row_state(pool, busy_message_uuid).await?;
    assert!(
        !busy_claimed,
        "the busy recipient's row must remain unclaimed after the reaper pass"
    );
    assert!(
        !busy_delivered,
        "the busy recipient's row must remain undelivered after the reaper pass"
    );
    Ok(())
}

async fn assert_busy_recipient_delivered(
    pool: &PgPool,
    mock: &MockServer,
    busy_message_uuid: uuid::Uuid,
    busy_text: &str,
) -> anyhow::Result<()> {
    let (busy_claimed, busy_delivered) = mailbox_row_state(pool, busy_message_uuid).await?;
    assert!(
        busy_claimed,
        "the busy recipient's row must be claimed once a primary turn delivers it"
    );
    assert!(
        busy_delivered,
        "the busy recipient's row must have sent_request_id stamped once delivered"
    );

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    let delivery_turn = received
        .last()
        .ok_or_else(|| anyhow::anyhow!("no forwarded request for the delivery turn"))?;
    let forwarded: Value = serde_json::from_slice(&delivery_turn.body)?;
    assert!(
        last_block_text(&forwarded)?.contains(busy_text),
        "the delivery turn must carry the full surviving mailbox envelope, un-truncated"
    );
    Ok(())
}

#[tokio::test]
async fn busy_recipient_survives_ttl_reaper_then_delivers_on_next_primary_turn()
-> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping busy_recipient_survives_ttl_reaper_then_delivers_on_next_primary_turn"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&db.database_url)
        .await?;

    let (state, proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let busy_session = uuid::Uuid::new_v4();
    let busy_session_str = busy_session.to_string();
    send_request(
        &client,
        &proxy_url,
        &busy_session_str,
        &primary_body("init busy"),
    )
    .await?;
    let busy_session_id = wait_for_session_id(&pool, busy_session).await?;

    let silent_session = uuid::Uuid::new_v4();
    seed_silent_session(&pool, silent_session).await?;

    let busy_text = "gate-pin: 323 subagent-lane requests, zero primary turns";
    let busy_message_uuid =
        queue_mailbox_message(&client, &inspect_url, busy_session, busy_text).await?;
    let silent_text = "gate-pin: never delivered, never reaped without the #130 fix";
    let silent_message_uuid =
        queue_mailbox_message(&client, &inspect_url, silent_session, silent_text).await?;

    backdate_queued_at(&pool, busy_message_uuid).await?;
    backdate_queued_at(&pool, silent_message_uuid).await?;

    seed_captured_request(
        &pool,
        busy_session_id,
        "subagent",
        Utc::now() - chrono::Duration::minutes(5),
    )
    .await?;

    mike::mailbox_reaper::spawn_mailbox_reaper(&state);

    assert_silent_companion_reaped_with_failed_send(&pool, silent_message_uuid, silent_session)
        .await?;
    assert_busy_recipient_untouched_by_reaper(&pool, busy_message_uuid, busy_session).await?;

    send_request(
        &client,
        &proxy_url,
        &busy_session_str,
        &primary_body("continue busy"),
    )
    .await?;

    assert_busy_recipient_delivered(&pool, &mock, busy_message_uuid, busy_text).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
