use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use super::support::{
    TestDb, ack_mailbox_message, boot_proxy, mailbox_row_state, ok_response, primary_body,
    queue_mailbox_message, send_request, wait_for_session_row,
};

#[tokio::test]
async fn ack_requires_delivery_then_is_idempotent_once_delivered() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping ack_requires_delivery_then_is_idempotent_once_delivered"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (_state, proxy_url, inspect_url, shutdown) =
        boot_proxy(&mock.uri(), &db.database_url, "").await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_str = session.to_string();
    send_request(&client, &proxy_url, &session_str, &primary_body("init")).await?;
    wait_for_session_row(&pool, session).await?;

    let message_uuid =
        queue_mailbox_message(&client, &inspect_url, session, "please ack me").await?;

    let (status, body) = ack_mailbox_message(&client, &inspect_url, session, message_uuid).await?;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"], "not_yet_delivered", "{body:?}");
    let (_claimed, delivered, consumed) = mailbox_row_state(&pool, message_uuid).await?;
    assert!(!delivered, "row must still be undelivered");
    assert!(
        !consumed,
        "a refused ack must not set consumed_at: {body:?}"
    );

    send_request(&client, &proxy_url, &session_str, &primary_body("continue")).await?;
    let (_claimed, delivered, _consumed) = mailbox_row_state(&pool, message_uuid).await?;
    assert!(delivered, "row must be delivered after a primary turn");

    let (status, body) = ack_mailbox_message(&client, &inspect_url, session, message_uuid).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body:?}");
    let first_consumed_at = body["consumed_at"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing consumed_at: {body:?}"))?
        .to_string();

    let (status, body) = ack_mailbox_message(&client, &inspect_url, session, message_uuid).await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{body:?}");
    assert_eq!(
        body["consumed_at"], first_consumed_at,
        "a repeat ack must be idempotent: {body:?}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
