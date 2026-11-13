mod fixtures;

use crate::test_db;

use fixtures::{
    DEFINITELY_STALE_AGE, boot_state, fetch_session_last_status, fetch_status,
    insert_pending_request, insert_pending_request_aged, set_session_last_status,
};
use mike::config::ProxyConfig;
use mike::state::AppState;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn recover_stale_pending_rows_updates_status_and_count() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping recover_stale_pending_rows_updates_status_and_count"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let request_id: i64 = 9_800_001;
    insert_pending_request_aged(&pool, request_id, DEFINITELY_STALE_AGE).await?;

    let pre_status = fetch_status(&pool, request_id).await?;
    assert_eq!(
        pre_status, "Pending",
        "fixture must seed the row as Pending before recovery"
    );

    let state = boot_state(&test_db.database_url).await?;
    mike::startup::run_crash_recovery(&state).await;

    let post_status = fetch_status(&pool, request_id).await?;
    assert_eq!(
        post_status, "Error: proxy_crashed_during_stream",
        "crash-recovery sweep must close Pending rows with 'Error: proxy_crashed_during_stream', got: {post_status:?}"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn run_crash_recovery_is_noop_under_smoke_run() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping run_crash_recovery_is_noop_under_smoke_run");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let request_id: i64 = 9_800_005;
    insert_pending_request(&pool, request_id).await?;

    let pre_status = fetch_status(&pool, request_id).await?;
    assert_eq!(
        pre_status, "Pending",
        "fixture must seed the row as Pending before the smoke boot"
    );

    let config = ProxyConfig::try_parse_from([
        "mike",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &test_db.database_url,
        "--smoke-run",
        "1",
    ])?;
    let state = AppState::new(config).await?;
    mike::startup::run_crash_recovery(&state).await;

    let post_status = fetch_status(&pool, request_id).await?;
    assert_eq!(
        post_status, "Pending",
        "smoke_run must leave a live in-flight row untouched — a throwaway \
         smoke boot has no business force-closing another process's request, \
         got: {post_status:?}"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recover_stale_pending_rows_leaves_complete_row_unchanged() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping recover_stale_pending_rows_leaves_complete_row_unchanged"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let pending_id_1: i64 = 9_800_002;
    let pending_id_2: i64 = 9_800_003;
    let complete_id: i64 = 9_800_004;

    insert_pending_request_aged(&pool, pending_id_1, DEFINITELY_STALE_AGE).await?;
    insert_pending_request_aged(&pool, pending_id_2, DEFINITELY_STALE_AGE).await?;
    insert_pending_request(&pool, complete_id).await?;

    sqlx::query(
        "UPDATE requests.requests r
            SET status_id = rs.id, completed_at = NOW()
          FROM requests.statuses rs
         WHERE rs.code = 'complete' AND r.id = $1",
    )
    .bind(complete_id)
    .execute(&pool)
    .await?;

    let state = boot_state(&test_db.database_url).await?;
    mike::startup::run_crash_recovery(&state).await;

    assert_eq!(
        fetch_status(&pool, pending_id_1).await?,
        "Error: proxy_crashed_during_stream",
        "first Pending row must be closed"
    );
    assert_eq!(
        fetch_status(&pool, pending_id_2).await?,
        "Error: proxy_crashed_during_stream",
        "second Pending row must be closed"
    );
    assert_eq!(
        fetch_status(&pool, complete_id).await?,
        "Complete",
        "Complete row must not be touched by crash-recovery sweep"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recover_stale_pending_rows_reconciles_session_last_status() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping recover_stale_pending_rows_reconciles_session_last_status"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let request_id: i64 = 9_800_010;
    insert_pending_request(&pool, request_id).await?;
    sqlx::query(
        "UPDATE requests.requests r
            SET status_id = rs.id, completed_at = NOW()
          FROM requests.statuses rs
         WHERE rs.code = 'error' AND r.id = $1",
    )
    .bind(request_id)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO requests.errors (request_id, message)
         VALUES ($1, 'proxy_crashed_during_stream')",
    )
    .bind(request_id)
    .execute(&pool)
    .await?;
    set_session_last_status(&pool, request_id, "Pending").await?;

    assert_eq!(
        fetch_session_last_status(&pool, request_id)
            .await?
            .as_deref(),
        Some("Pending"),
        "fixture must leave the session drifted at 'Pending' before recovery"
    );

    let state = boot_state(&test_db.database_url).await?;
    mike::startup::run_crash_recovery(&state).await;

    assert_eq!(
        fetch_session_last_status(&pool, request_id)
            .await?
            .as_deref(),
        Some("Error: proxy_crashed_during_stream"),
        "crash-recovery must reconcile sessions.last_status to the newest request's terminal status"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn recover_stale_pending_rows_ignores_a_fresh_row_from_another_live_instance()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping recover_stale_pending_rows_ignores_a_fresh_row_from_another_live_instance"
        );
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let fresh_id: i64 = 9_800_020;
    insert_pending_request_aged(&pool, fresh_id, chrono::Duration::zero()).await?;

    let stale_id: i64 = 9_800_021;
    insert_pending_request_aged(&pool, stale_id, DEFINITELY_STALE_AGE).await?;

    assert_eq!(
        fetch_status(&pool, fresh_id).await?,
        "Pending",
        "fixture must seed the fresh row as Pending before the sweep"
    );
    assert_eq!(
        fetch_status(&pool, stale_id).await?,
        "Pending",
        "fixture must seed the stale row as Pending before the sweep"
    );

    let state = boot_state(&test_db.database_url).await?;
    mike::startup::run_crash_recovery(&state).await;

    assert_eq!(
        fetch_status(&pool, fresh_id).await?,
        "Pending",
        "a row seconds old must survive the sweep untouched — it may belong \
         to another live instance's genuinely in-flight request (#219)"
    );
    assert_eq!(
        fetch_status(&pool, stale_id).await?,
        "Error: proxy_crashed_during_stream",
        "a row old enough that no genuine request could still own it must \
         still be recovered"
    );

    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
