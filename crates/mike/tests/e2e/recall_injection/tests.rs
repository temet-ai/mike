use crate::test_db;

use super::db::{insert_lesson_with_topic, insert_project, project_id_for_path};
use super::fixtures::{body_with_recall_tag, ok_response, send_request};
use super::harness::{boot_proxy_recall_off, boot_proxy_recall_on};

use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use test_db::TestDb;

#[tokio::test]
async fn recall_injection_end_to_end() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping recall_injection_end_to_end");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    sqlx::query(
        "INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES ('recall_injector', true, true)
         ON CONFLICT (key) DO UPDATE SET enabled = true, operator_set = true",
    )
    .execute(&pool)
    .await?;

    let (_state, proxy_url, shutdown) = boot_proxy_recall_on(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let working_dir = "/Users/tester/projects/recall-e2e";
    let lesson_text = "ALWAYS use make targets, never raw cargo nextest in this repo";

    let project_id = insert_project(&pool, working_dir).await?;
    assert_eq!(
        project_id_for_path(&pool, working_dir).await?,
        Some(project_id),
        "the seeded lesson's project must be reachable by the path recall keys on"
    );

    insert_lesson_with_topic(&pool, project_id, "warning", lesson_text, "make-targets").await?;

    let session_b = uuid::Uuid::new_v4().to_string();
    send_request(
        &client,
        &proxy_url,
        &session_b,
        &body_with_recall_tag(working_dir, "make-targets", "ok continue"),
    )
    .await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 1, "expected 1 forwarded request (recall)");

    let forwarded: Value = serde_json::from_slice(&received[0].body)?;
    let messages = forwarded["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("forwarded body missing messages array"))?;

    let last_msg = messages
        .last()
        .ok_or_else(|| anyhow::anyhow!("messages array is empty"))?;
    assert_eq!(
        last_msg["role"].as_str().unwrap_or(""),
        "user",
        "last message must be user"
    );

    let content = last_msg["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("last message content is not an array"))?;
    let last_block = content
        .last()
        .ok_or_else(|| anyhow::anyhow!("last message content is empty"))?;
    let block_text = last_block["text"].as_str().unwrap_or("");

    assert!(
        block_text.contains("<system-reminder>"),
        "forwarded body must carry <system-reminder> block; got: {block_text}"
    );
    assert!(
        block_text.contains(lesson_text),
        "forwarded body must carry full lesson text (no truncation); got: {block_text}"
    );
    assert!(
        block_text.contains("[topic: make-targets]"),
        "forwarded body must include topic label; got: {block_text}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_injection_disabled_forwards_body_unchanged() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping recall_injection_disabled_forwards_body_unchanged"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    sqlx::query(
        "INSERT INTO config.feature_toggles (key, enabled) VALUES ('recall_injector', false)
         ON CONFLICT (key) DO UPDATE SET enabled = EXCLUDED.enabled",
    )
    .execute(&pool)
    .await?;

    let (_state, proxy_url, shutdown) =
        boot_proxy_recall_off(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let working_dir = "/Users/tester/projects/recall-disabled";

    let project_id = insert_project(&pool, working_dir).await?;
    insert_lesson_with_topic(
        &pool,
        project_id,
        "warning",
        "should not appear",
        "any-topic",
    )
    .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    send_request(
        &client,
        &proxy_url,
        &session_id,
        &body_with_recall_tag(working_dir, "any-topic", "ask me something"),
    )
    .await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 1, "expected exactly 1 forwarded request");

    let forwarded: Value = serde_json::from_slice(&received[0].body)?;
    let messages = forwarded["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages must be array"))?;
    let last_msg = messages
        .last()
        .ok_or_else(|| anyhow::anyhow!("messages must be non-empty"))?;
    let content = last_msg["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    let last_block = content
        .last()
        .ok_or_else(|| anyhow::anyhow!("content must be non-empty"))?;
    let block_text = last_block["text"].as_str().unwrap_or("");

    assert!(
        !block_text.contains("<system-reminder>"),
        "disabled recall injector must NOT append a system-reminder; got: {block_text}"
    );
    assert!(
        !block_text.contains("should not appear"),
        "disabled recall injector must NOT inject lesson text; got: {block_text}"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recall_injection_no_matching_lesson_forwards_unchanged() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping recall_injection_no_matching_lesson_forwards_unchanged"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (_state, proxy_url, shutdown) = boot_proxy_recall_on(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    let working_dir = "/Users/tester/projects/recall-no-match";

    insert_project(&pool, working_dir).await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let body = body_with_recall_tag(working_dir, "ghost-topic", "no match expected");
    let raw_body_bytes = serde_json::to_vec(&body)?;
    send_request(&client, &proxy_url, &session_id, &body).await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    assert_eq!(received.len(), 1, "expected exactly 1 forwarded request");

    let forwarded: Value = serde_json::from_slice(&received[0].body)?;
    let messages = forwarded["messages"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("messages must be array"))?;
    let last_msg = messages
        .last()
        .ok_or_else(|| anyhow::anyhow!("messages must be non-empty"))?;
    let content = last_msg["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    let last_block = content
        .last()
        .ok_or_else(|| anyhow::anyhow!("content must be non-empty"))?;
    let block_text = last_block["text"].as_str().unwrap_or("");

    assert!(
        !block_text.contains("<system-reminder>"),
        "no matching lesson must not produce a system-reminder; got: {block_text}"
    );

    let original: Value = serde_json::from_slice(&raw_body_bytes)?;
    assert_eq!(
        forwarded["messages"], original["messages"],
        "messages array must be byte-identical when no match found"
    );

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
