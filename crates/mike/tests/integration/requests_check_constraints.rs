use crate::test_db;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

async fn insert_request(
    pool: &PgPool,
    method: &str,
    status: &str,
    turn_type: &str,
    turn_origin: &str,
) -> sqlx::Result<()> {
    let project_id: i64 = sqlx::query_scalar(
        "WITH ins AS (
             INSERT INTO projects.projects
                 (project_path, display_name, first_seen, last_seen)
             SELECT $1, $1, NOW(), NOW()
             WHERE NOT EXISTS (
                 SELECT 1 FROM projects.projects WHERE project_path = $1
             )
             RETURNING id
         )
         SELECT id FROM ins
         UNION ALL
         SELECT id FROM projects.projects WHERE project_path = $1
         LIMIT 1",
    )
    .bind("h2-test-project")
    .fetch_one(pool)
    .await?;

    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions
                (session_uuid, project_id, first_seen, last_seen)
              VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(session_uuid)
    .bind(project_id)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO requests.requests
                (session_id, project_id,
                 method_id, path,
                 status_id, turn_type_id, turn_origin_id,
                 is_streaming, message_count, is_tool_loop,
                 params, last_user_message, human_prompt,
                 created_at)
              VALUES (
                      (SELECT id FROM sessions.sessions WHERE session_uuid = $1),
                      $2,
                      (SELECT id FROM requests.methods      WHERE code = $3),
                      '/v1/messages',
                      (SELECT id FROM requests.statuses     WHERE code = $4),
                      (SELECT id FROM requests.turn_types   WHERE code = $5),
                      (SELECT id FROM requests.turn_origins WHERE code = $6),
                      FALSE, 0, FALSE,
                      '{}'::jsonb, '', '',
                      NOW())",
    )
    .bind(session_uuid)
    .bind(project_id)
    .bind(method)
    .bind(status)
    .bind(turn_type)
    .bind(turn_origin)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn baseline_accepted_values_succeed() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping baseline_accepted_values_succeed");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    insert_request(&pool, "POST", "pending", "user_prompt", "primary").await?;
    insert_request(&pool, "GET", "streaming", "tool_loop", "subagent").await?;
    insert_request(&pool, "DELETE", "complete", "continuation", "title_sidecar").await?;
    insert_request(&pool, "PATCH", "error", "user_prompt", "quota_probe").await?;

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn method_rejects_lowercase() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping method_rejects_lowercase");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let err = match insert_request(&pool, "post", "pending", "user_prompt", "primary").await {
        Err(e) => e,
        Ok(()) => panic!("lowercase method must be rejected"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("method_id"),
        "expected method_id NOT NULL violation, got: {msg}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn turn_type_rejects_unknown_value() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping turn_type_rejects_unknown_value");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let err = match insert_request(&pool, "POST", "pending", "user_prompt_typo", "primary").await {
        Err(e) => e,
        Ok(()) => panic!("unknown turn_type must be rejected"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("turn_type_id"),
        "expected turn_type_id NOT NULL violation, got: {msg}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn turn_origin_rejects_trailing_underscore_typo() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping turn_origin_rejects_trailing_underscore_typo"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let err = match insert_request(&pool, "POST", "pending", "user_prompt", "subagent_").await {
        Err(e) => e,
        Ok(()) => panic!("turn_origin trailing underscore must be rejected"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("turn_origin_id"),
        "expected turn_origin_id NOT NULL violation, got: {msg}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn status_rejects_uppercase_pending() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping status_rejects_uppercase_pending");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let err = match insert_request(&pool, "POST", "Pending", "user_prompt", "primary").await {
        Err(e) => e,
        Ok(()) => panic!("uppercase status must be rejected"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("status_id"),
        "expected status_id NOT NULL violation, got: {msg}"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn status_rejects_unknown_code() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping status_rejects_unknown_code");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let err = match insert_request(
        &pool,
        "POST",
        "unknown_status_xyz",
        "user_prompt",
        "primary",
    )
    .await
    {
        Err(e) => e,
        Ok(()) => panic!("unknown status code must be rejected"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("status_id"),
        "expected status_id NOT NULL violation, got: {msg}"
    );

    test_db.cleanup().await?;
    Ok(())
}
