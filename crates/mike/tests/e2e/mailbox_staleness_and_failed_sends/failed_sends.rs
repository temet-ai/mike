use serde_json::json;
use sqlx::postgres::PgPoolOptions;

use super::support::{TestDb, boot_proxy, count_failed_sends, seed_session};

#[tokio::test]
async fn empty_message_send_is_captured_in_failed_sends() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping empty_message_send_is_captured_in_failed_sends"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;

    let resp = client
        .post(format!("{inspect_url}/api/sessions/{session}/message"))
        .json(&json!({"message": "   "}))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);

    assert_eq!(
        count_failed_sends(&pool, &session.to_string(), "empty_message").await?,
        1,
        "an empty-message send must land a durable failed_sends row"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_from_session_id_send_is_captured_in_failed_sends() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping invalid_from_session_id_send_is_captured_in_failed_sends"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;

    let resp = client
        .post(format!("{inspect_url}/api/sessions/{session}/message"))
        .json(&json!({"message": "hello", "from_session_id": "not-a-uuid"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);

    assert_eq!(
        count_failed_sends(&pool, &session.to_string(), "invalid_from_session_id").await?,
        1,
        "a malformed from_session_id must land a durable failed_sends row"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_recipient_send_is_captured_in_failed_sends() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping unknown_recipient_send_is_captured_in_failed_sends"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let bogus_recipient = uuid::Uuid::new_v4();

    let resp = client
        .post(format!(
            "{inspect_url}/api/sessions/{bogus_recipient}/message"
        ))
        .json(&json!({"message": "is anyone there"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    assert_eq!(
        count_failed_sends(&pool, &bogus_recipient.to_string(), "session_not_found").await?,
        1,
        "an unknown-recipient send must land a durable, queryable failed_sends row"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_from_session_send_is_captured_in_failed_sends() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping unknown_from_session_send_is_captured_in_failed_sends"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;
    let unknown_from = uuid::Uuid::new_v4();

    let resp = client
        .post(format!("{inspect_url}/api/sessions/{session}/message"))
        .json(
            &json!({"message": "reply pointer test", "from_session_id": unknown_from.to_string()}),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 404);

    assert_eq!(
        count_failed_sends(&pool, &session.to_string(), "from_session_not_found").await?,
        1,
        "an unresolvable from_session_id must land a durable failed_sends row"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
