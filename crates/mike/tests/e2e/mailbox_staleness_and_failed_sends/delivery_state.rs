use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;

use super::support::{
    TestDb, boot_proxy_with_delivery_ttl, get_messages, queue_message, seed_request, seed_session,
};

#[tokio::test]
async fn pending_row_with_recent_recipient_activity_reports_awaiting_eligible_turn()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping pending_row_with_recent_recipient_activity_reports_awaiting_eligible_turn"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy_with_delivery_ttl(&db.database_url, 10, 60).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;
    seed_request(&pool, session, "subagent", 2).await?;
    seed_request(&pool, session, "primary", 1).await?;

    let message_uuid = queue_message(&client, &inspect_url, session, "are you there").await?;

    let body = get_messages(&client, &inspect_url, session).await?;
    let row = &body["inbox"][0];
    assert_eq!(row["message_uuid"], message_uuid.to_string());
    assert_eq!(row["status"], "pending");
    assert_eq!(row["delivery_state"], "awaiting_eligible_turn");
    assert!(
        row["recipient_last_request_at"].is_string(),
        "recipient_last_request_at must be populated: {row:?}"
    );
    assert!(
        row["recipient_last_eligible_turn_at"].is_string(),
        "recipient_last_eligible_turn_at must be populated from the primary-lane request: {row:?}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn pending_row_with_stale_or_no_recipient_activity_reports_recipient_inactive()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping pending_row_with_stale_or_no_recipient_activity_reports_recipient_inactive"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy_with_delivery_ttl(&db.database_url, 10, 5).await?;
    let client = reqwest::Client::new();

    let stale_session = uuid::Uuid::new_v4();
    seed_session(&pool, stale_session).await?;
    seed_request(&pool, stale_session, "primary", 30).await?;
    let stale_message_uuid =
        queue_message(&client, &inspect_url, stale_session, "still there?").await?;

    let body = get_messages(&client, &inspect_url, stale_session).await?;
    let row = &body["inbox"][0];
    assert_eq!(row["message_uuid"], stale_message_uuid.to_string());
    assert_eq!(row["delivery_state"], "recipient_inactive");
    assert!(
        row["recipient_last_request_at"].is_string(),
        "the stale timestamp is still reported, just outside the TTL: {row:?}"
    );

    let silent_session = uuid::Uuid::new_v4();
    seed_session(&pool, silent_session).await?;
    let silent_message_uuid =
        queue_message(&client, &inspect_url, silent_session, "anyone home?").await?;

    let body = get_messages(&client, &inspect_url, silent_session).await?;
    let row = &body["inbox"][0];
    assert_eq!(row["message_uuid"], silent_message_uuid.to_string());
    assert_eq!(row["delivery_state"], "recipient_inactive");
    assert!(
        row["recipient_last_request_at"].is_null(),
        "a recipient with no captured traffic must report null, not a stale value: {row:?}"
    );
    assert!(row["recipient_last_eligible_turn_at"].is_null());

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delivered_row_reports_null_delivery_state() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping delivered_row_reports_null_delivery_state");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy_with_delivery_ttl(&db.database_url, 10, 60).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;
    let message_uuid = queue_message(&client, &inspect_url, session, "delivered already").await?;

    seed_request(&pool, session, "primary", 0).await?;
    let request_id: i64 =
        sqlx::query_scalar("SELECT id FROM requests.requests WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1) ORDER BY id DESC LIMIT 1")
            .bind(session)
            .fetch_one(&pool)
            .await?;
    sqlx::query(
        "UPDATE mailbox.messages SET sent_request_id = $2, sent_at = now() WHERE message_uuid = $1",
    )
    .bind(message_uuid)
    .bind(request_id)
    .execute(&pool)
    .await?;

    let body = get_messages(&client, &inspect_url, session).await?;
    let row = &body["inbox"][0];
    assert_eq!(row["status"], "delivered");
    assert!(
        row["delivery_state"].is_null(),
        "a delivered row must not carry an awaiting/inactive delivery_state: {row:?}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn post_message_response_carries_recipient_delivery_state() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping post_message_response_carries_recipient_delivery_state"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy_with_delivery_ttl(&db.database_url, 10, 60).await?;
    let client = reqwest::Client::new();

    let active_session = uuid::Uuid::new_v4();
    seed_session(&pool, active_session).await?;
    seed_request(&pool, active_session, "primary", 1).await?;

    let resp = client
        .post(format!(
            "{inspect_url}/api/sessions/{active_session}/message"
        ))
        .json(&json!({"message": "hello active recipient"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    assert_eq!(envelope["delivery_state"], "awaiting_eligible_turn");
    assert!(envelope["recipient_last_request_at"].is_string());
    assert!(envelope["recipient_last_eligible_turn_at"].is_string());

    let silent_session = uuid::Uuid::new_v4();
    seed_session(&pool, silent_session).await?;

    let resp = client
        .post(format!(
            "{inspect_url}/api/sessions/{silent_session}/message"
        ))
        .json(&json!({"message": "hello silent recipient"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    assert_eq!(envelope["delivery_state"], "recipient_inactive");
    assert!(envelope["recipient_last_request_at"].is_null());
    assert!(envelope["recipient_last_eligible_turn_at"].is_null());

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
