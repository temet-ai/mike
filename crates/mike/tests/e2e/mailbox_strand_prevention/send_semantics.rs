use sqlx::postgres::PgPoolOptions;

use super::support::{
    TestDb, boot_proxy, count_failed_sends, post_message, seed_registration, seed_session,
};

#[tokio::test]
async fn send_to_ended_session_is_rejected_with_409() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping send_to_ended_session_is_rejected_with_409");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (_state, _proxy_url, inspect_url, shutdown) =
        boot_proxy("http://127.0.0.1:1", &db.database_url, "").await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_id = seed_session(&pool, session).await?;
    let project_id: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&pool)
            .await?;
    seed_registration(&pool, session_id, project_id, 900, true).await?;

    let (status, body) = post_message(&client, &inspect_url, session, "is anyone there").await?;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"], "recipient_ended", "{body:?}");
    assert_eq!(
        count_failed_sends(&pool, &session.to_string(), "recipient_ended").await?,
        1,
        "a send rejected for an ended recipient must land a durable failed_sends row"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn send_to_live_session_queues_with_liveness_in_body() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping send_to_live_session_queues_with_liveness_in_body"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (_state, _proxy_url, inspect_url, shutdown) =
        boot_proxy("http://127.0.0.1:1", &db.database_url, "").await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_id = seed_session(&pool, session).await?;
    let project_id: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&pool)
            .await?;
    seed_registration(&pool, session_id, project_id, 900, false).await?;

    let (status, body) = post_message(&client, &inspect_url, session, "hello").await?;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body:?}");
    assert_eq!(body["recipient_liveness"], "live", "{body:?}");

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn send_to_stale_session_still_queues_201() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping send_to_stale_session_still_queues_201");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (_state, _proxy_url, inspect_url, shutdown) =
        boot_proxy("http://127.0.0.1:1", &db.database_url, "").await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    let session_id = seed_session(&pool, session).await?;
    let project_id: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&pool)
            .await?;
    seed_registration(&pool, session_id, project_id, -1200, false).await?;

    let (status, body) = post_message(&client, &inspect_url, session, "hello").await?;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body:?}");
    assert_eq!(body["recipient_liveness"], "stale", "{body:?}");

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn send_to_unregistered_session_still_queues_201() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping send_to_unregistered_session_still_queues_201"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (_state, _proxy_url, inspect_url, shutdown) =
        boot_proxy("http://127.0.0.1:1", &db.database_url, "").await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;

    let (status, body) = post_message(&client, &inspect_url, session, "hello").await?;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body:?}");
    assert!(
        body["recipient_liveness"].is_null(),
        "an unregistered session has no liveness signal: {body:?}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
