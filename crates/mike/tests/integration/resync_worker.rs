use crate::test_db;

use sqlx::AssertSqlSafe;
use std::sync::Arc;

use arc_swap::ArcSwap;
use mike::resync::worker::run_once;
use mike::resync::{ResyncStatus, ResyncStatusHandle};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

async fn connect(url: &str) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new().max_connections(4).connect(url).await?)
}

async fn count(pool: &PgPool, table: &str) -> anyhow::Result<i64> {
    let n: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await?;
    Ok(n)
}

async fn seed_minimal_session(pool: &PgPool) -> anyhow::Result<String> {
    let session_uuid = "5e550000-0000-4000-8000-0000000000aa".to_string();

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name)
         VALUES ('/home/user/worker-demo', 'worker-demo') RETURNING id",
    )
    .fetch_one(pool)
    .await?;

    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, title, last_status)
         VALUES ($1::uuid, $2, 'worker demo', 'complete') RETURNING id",
    )
    .bind(&session_uuid)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO requests.requests
            (session_id, project_id, method_id, path, model, is_streaming,
             message_count, is_tool_loop, status_id, params, last_user_message,
             human_prompt, turn_type_id, turn_origin_id, request_body)
         VALUES ($1, $2, 2, '/v1/messages', 'claude-x', true, 1, false, 3,
                 '{}'::jsonb, 'hi', 'hi', 1, 1, '\\xdeadbeef'::bytea)",
    )
    .bind(session_id)
    .bind(project_id)
    .execute(pool)
    .await?;

    Ok(session_uuid)
}

#[tokio::test]
async fn run_once_drains_reachable_dest_and_updates_status() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_worker integration test");
        return Ok(());
    };

    let source_handle = test_db.acquire_isolated().await?;
    let dest_handle = test_db.acquire_isolated().await?;
    let source = connect(&source_handle.database_url).await?;
    let dest = connect(&dest_handle.database_url).await?;

    let session_uuid = seed_minimal_session(&source).await?;

    let dest_url = dest_handle.database_url.clone();
    let status: ResyncStatusHandle = Arc::new(ArcSwap::from_pointee(ResyncStatus::default()));

    run_once(&source, &dest, &dest_url, &status).await?;

    assert_eq!(
        count(&dest, "projects.projects").await?,
        1,
        "project copied"
    );
    assert_eq!(
        count(&dest, "sessions.sessions").await?,
        1,
        "session copied"
    );
    assert_eq!(
        count(&dest, "requests.requests").await?,
        1,
        "request copied"
    );

    let joined_uuid: String = sqlx::query_scalar(
        "SELECT s.session_uuid::text
           FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          LIMIT 1",
    )
    .fetch_one(&dest)
    .await?;
    assert_eq!(
        joined_uuid, session_uuid,
        "request FK remapped to dest session"
    );

    let snap = status.load();
    assert!(snap.last_run_at.is_some(), "status records the run time");
    assert!(snap.dest_reachable, "dest was reachable this tick");
    assert!(
        snap.last_error.is_none(),
        "successful tick clears last_error"
    );
    assert!(
        snap.last_report.total_rows > 0,
        "status report carries the drained row count, got {}",
        snap.last_report.total_rows
    );
    assert!(
        snap.last_report
            .tables
            .iter()
            .any(|(t, _)| t == "requests.requests"),
        "per-table summary names requests.requests"
    );

    run_once(&source, &dest, &dest_url, &status).await?;
    assert_eq!(
        count(&dest, "requests.requests").await?,
        1,
        "second tick inserts no duplicate rows"
    );
    let snap2 = status.load();
    assert!(snap2.dest_reachable, "still reachable on second tick");
    assert_eq!(
        snap2.last_report.total_rows, 0,
        "second tick drains nothing (idempotent)"
    );

    source.close().await;
    dest.close().await;
    source_handle.cleanup().await?;
    dest_handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn run_once_unreachable_dest_marks_status_and_skips() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_worker unreachable test");
        return Ok(());
    };

    let source_handle = test_db.acquire_isolated().await?;
    let dest_handle = test_db.acquire_isolated().await?;
    let source = connect(&source_handle.database_url).await?;
    let dest = connect(&dest_handle.database_url).await?;

    seed_minimal_session(&source).await?;

    let unreachable_url = "postgres://u:p@127.0.0.1:1/claude_proxy_never".to_string();
    let status: ResyncStatusHandle = Arc::new(ArcSwap::from_pointee(ResyncStatus::default()));

    run_once(&source, &dest, &unreachable_url, &status).await?;

    let snap = status.load();
    assert!(snap.last_run_at.is_some(), "tick recorded a run time");
    assert!(!snap.dest_reachable, "dest must be marked unreachable");
    assert_eq!(
        count(&dest, "requests.requests").await?,
        0,
        "no rows drained when dest is unreachable"
    );

    source.close().await;
    dest.close().await;
    source_handle.cleanup().await?;
    dest_handle.cleanup().await?;
    Ok(())
}
