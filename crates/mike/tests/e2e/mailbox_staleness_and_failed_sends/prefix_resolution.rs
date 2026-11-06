use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;

use super::support::{TestDb, boot_proxy, count_failed_sends, queue_message, seed_session};

#[tokio::test]
async fn unique_id_prefix_resolves_to_the_session() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping unique_id_prefix_resolves_to_the_session");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let session: uuid::Uuid = "44e5d7ab-0000-4000-8000-000000000001".parse()?;
    seed_session(&pool, session).await?;

    let resp = client
        .post(format!("{inspect_url}/api/sessions/44e5d7ab/message"))
        .json(&json!({"message": "hello via short id"}))
        .send()
        .await?;
    let status = resp.status();
    let envelope: Value = resp.json().await?;
    assert_eq!(status, 201, "{envelope:?}");
    assert_eq!(
        envelope["to_session_uuid"],
        session.to_string(),
        "the prefix must resolve to the one seeded session, not an unrelated match"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn ambiguous_id_prefix_returns_409_with_candidates() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping ambiguous_id_prefix_returns_409_with_candidates"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let session_a: uuid::Uuid = "44e5d7ab-1111-4111-8111-111111111111".parse()?;
    let session_b: uuid::Uuid = "44e5d7ab-2222-4222-8222-222222222222".parse()?;
    seed_session(&pool, session_a).await?;
    seed_session(&pool, session_b).await?;

    let resp = client
        .post(format!("{inspect_url}/api/sessions/44e5d7ab/message"))
        .json(&json!({"message": "which one am I reaching"}))
        .send()
        .await?;
    let status = resp.status();
    let envelope: Value = resp.json().await?;
    assert_eq!(status, 409, "{envelope:?}");
    assert_eq!(envelope["error"], "ambiguous_session_id");
    let message = envelope["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(&session_a.to_string()) && message.contains(&session_b.to_string()),
        "ambiguous response must list every candidate: {message}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn zero_match_id_prefix_returns_404() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping zero_match_id_prefix_returns_404");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{inspect_url}/api/sessions/deadbeef/message"))
        .json(&json!({"message": "is anyone at this prefix"}))
        .send()
        .await?;
    let status = resp.status();
    let envelope: Value = resp.json().await?;
    assert_eq!(status, 404, "{envelope:?}");
    assert_eq!(envelope["error"], "session_not_found");
    assert!(
        envelope["message"]
            .as_str()
            .unwrap_or_default()
            .contains("id prefix"),
        "{envelope:?}"
    );
    assert_eq!(
        count_failed_sends(&pool, "deadbeef", "session_not_found").await?,
        1,
        "a zero-match prefix send must land a durable failed_sends row"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn non_hex_id_returns_400_invalid_session_id() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping non_hex_id_returns_400_invalid_session_id");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{inspect_url}/api/sessions/nothexzz/message"))
        .json(&json!({"message": "not a hex prefix"}))
        .send()
        .await?;
    let status = resp.status();
    let envelope: Value = resp.json().await?;
    assert_eq!(status, 400, "{envelope:?}");
    assert_eq!(envelope["error"], "invalid_session_id");

    shutdown.cancel();
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn full_uuid_send_unchanged_and_404_carries_hint() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping full_uuid_send_unchanged_and_404_carries_hint"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (inspect_url, shutdown) = boot_proxy(&db.database_url, 10).await?;
    let client = reqwest::Client::new();

    let session = uuid::Uuid::new_v4();
    seed_session(&pool, session).await?;

    let message_uuid = queue_message(&client, &inspect_url, session, "still works").await?;
    assert!(!message_uuid.is_nil());

    let unknown = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{inspect_url}/api/sessions/{unknown}/message"))
        .json(&json!({"message": "anyone?"}))
        .send()
        .await?;
    let status = resp.status();
    let envelope: Value = resp.json().await?;
    assert_eq!(status, 404, "{envelope:?}");
    assert_eq!(envelope["error"], "session_not_found");
    let message = envelope["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("do not pad short ids with zeros"),
        "{message}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
