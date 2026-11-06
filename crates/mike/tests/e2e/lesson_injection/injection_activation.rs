use crate::test_db;

use mike::config::AppState;
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use test_db::TestDb;

use super::support::{
    body_with_working_dir, boot_proxy, create_project, insert_lesson, ok_response,
    project_id_for_path, send_request,
};

fn last_block_text(body: &Value) -> anyhow::Result<&str> {
    body["messages"]
        .as_array()
        .and_then(|msgs| msgs.last())
        .and_then(|m| m["content"].as_array())
        .and_then(|content| content.last())
        .and_then(|b| b["text"].as_str())
        .ok_or_else(|| anyhow::anyhow!("forwarded body missing appended content block: {body}"))
}

async fn run_first_request_scenario(
    state: &AppState,
    pool: &PgPool,
    client: &reqwest::Client,
    proxy_url: &str,
    mock: &MockServer,
) -> anyhow::Result<()> {
    let working_dir = "/Users/tester/projects/lesson-inject-demo";
    let lesson_text = "ALWAYS run make targets, never raw cargo nextest in this repo";

    let session_a = uuid::Uuid::new_v4().to_string();
    send_request(
        client,
        proxy_url,
        &session_a,
        &body_with_working_dir(working_dir, "first turn"),
    )
    .await?;

    let project_id = create_project(pool, working_dir).await?;
    assert_eq!(
        project_id_for_path(pool, working_dir).await?,
        Some(project_id),
        "the seeded lesson's project must be reachable by the path injection keys on"
    );

    insert_lesson(pool, project_id, "correction", lesson_text, "daily").await?;
    mike::lesson_worker::refresh_lesson_cache(state).await;

    let snap = state.injectable_lessons(working_dir);
    assert_eq!(snap.len(), 1, "snapshot must hold the seeded lesson");
    assert_eq!(snap[0].text, lesson_text);

    let session_b = uuid::Uuid::new_v4().to_string();
    send_request(
        client,
        proxy_url,
        &session_b,
        &body_with_working_dir(working_dir, "run cargo nextest in this repo"),
    )
    .await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    let forwarded: Value = serde_json::from_slice(
        &received
            .last()
            .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?
            .body,
    )?;
    let text = last_block_text(&forwarded)?;
    assert!(
        text.contains(lesson_text),
        "gate-4: the real project's lesson MUST appear in the forwarded body, got: {text}"
    );
    assert!(
        text.contains("<system-reminder source=\"proxy-lessons\">"),
        "forwarded body must carry the proxy-lessons reminder block, got: {text}"
    );
    Ok(())
}

async fn run_legacy_and_canonical_scenario(
    state: &AppState,
    pool: &PgPool,
    client: &reqwest::Client,
    proxy_url: &str,
    mock: &MockServer,
) -> anyhow::Result<()> {
    let working_dir = "/Users/tester/projects/split-key-demo";
    let legacy_key = "projects/split-key-demo";
    let canonical_lesson = "canonical row lesson: prefer make targets";
    let legacy_lesson = "legacy row lesson: never edit the generated file";

    let canonical_id = create_project(pool, working_dir).await?;
    let legacy_id = create_project(pool, legacy_key).await?;
    assert_ne!(
        canonical_id, legacy_id,
        "the split this test reproduces requires two distinct project rows"
    );
    insert_lesson(pool, canonical_id, "correction", canonical_lesson, "daily").await?;
    insert_lesson(pool, legacy_id, "convention", legacy_lesson, "daily").await?;
    mike::lesson_worker::refresh_lesson_cache(state).await;

    let session = uuid::Uuid::new_v4().to_string();
    send_request(
        client,
        proxy_url,
        &session,
        &body_with_working_dir(working_dir, "make targets and edit the generated file"),
    )
    .await?;

    let received = mock
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?;
    let forwarded: Value = serde_json::from_slice(
        &received
            .last()
            .ok_or_else(|| anyhow::anyhow!("wiremock recorded no requests"))?
            .body,
    )?;
    let text = last_block_text(&forwarded)?;

    assert!(
        text.contains(canonical_lesson),
        "the canonical row's lesson must be injected, got: {text}"
    );
    assert!(
        text.contains(legacy_lesson),
        "the legacy-key row's lesson must be injected too, got: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn lessons_injected_first_request_and_from_legacy_and_canonical_rows() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping lessons_injected_first_request_and_from_legacy_and_canonical_rows"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ($1, $2::text::jsonb)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind("reminders.inject_lessons")
    .bind("true")
    .execute(&pool)
    .await?;

    let (state, proxy_url, shutdown) = boot_proxy(&mock.uri(), &db.database_url).await?;
    let client = reqwest::Client::new();

    run_first_request_scenario(&state, &pool, &client, &proxy_url, &mock).await?;
    run_legacy_and_canonical_scenario(&state, &pool, &client, &proxy_url, &mock).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
