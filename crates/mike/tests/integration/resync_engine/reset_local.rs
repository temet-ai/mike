use crate::test_db;

use super::support::{connect, count};
use test_db::TestDb;

#[tokio::test]
async fn reset_local_seeded_synced_data_drops_rows_and_remigrates_to_same_version()
-> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_engine reset_local test");
        return Ok(());
    };

    let handle = test_db.acquire_isolated().await?;
    let pool = connect(&handle.database_url).await?;

    let version_before: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;

    seed_synced_rows(&pool).await?;

    mike::resync::reset_local(&pool).await?;

    assert_reset(&pool, version_before).await?;
    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

async fn seed_synced_rows(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let pool = pool.clone();
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name)
         VALUES ('/home/user/reset-demo', 'reset-demo') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, title, last_status)
         VALUES (gen_random_uuid(), $1, 'reset session', 'complete') RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO sync.cursor (table_name, last_local_id) VALUES ('sessions.sessions', $1)",
    )
    .bind(session_id)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO sync.id_map (table_name, local_id, remote_id) VALUES ('sessions.sessions', $1, $1)",
    )
    .bind(session_id)
    .execute(&pool)
    .await?;

    assert!(
        count(&pool, "steering.steerers").await? > 0,
        "precondition: migration-seeded steerer roster must exist before reset"
    );

    sqlx::query(
        "INSERT INTO eval.bundles (role, origin, status, components, content_sha256, build_sha)
         VALUES ('turn_judge', 'candidate', 'candidate', '{}'::jsonb, 'reset-demo', 'reset-demo')",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO runtime.namespaces (id, runtime_kind, durable_key, identity)
         VALUES (gen_random_uuid(), 'claude', 'reset-demo', '{}'::jsonb)",
    )
    .execute(&pool)
    .await?;
    Ok(())
}

async fn assert_reset(pool: &sqlx::PgPool, version_before: i64) -> anyhow::Result<()> {
    let pool = pool.clone();
    let version_after: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        version_after, version_before,
        "reset must re-migrate to the same embedded migrator version"
    );

    assert!(
        count(&pool, "steering.steerers").await? > 0,
        "reset must re-seed the steerer roster"
    );
    let primary_origin: i64 =
        sqlx::query_scalar("SELECT count(*) FROM requests.turn_origins WHERE code = 'primary'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        primary_origin, 1,
        "reset must re-seed requests.turn_origins 'primary'"
    );
    assert_dropped(&pool).await
}

async fn assert_dropped(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let pool = pool.clone();
    assert_eq!(
        count(&pool, "projects.projects").await?,
        0,
        "reset must drop seeded project rows"
    );
    assert_eq!(
        count(&pool, "sessions.sessions").await?,
        0,
        "reset must drop seeded session rows"
    );

    assert_eq!(
        count(&pool, "sync.id_map").await?,
        0,
        "reset must drop sync.id_map rows"
    );
    assert_eq!(
        count(&pool, "sync.cursor").await?,
        0,
        "reset must drop sync.cursor rows"
    );

    let leftover_bundles: i64 =
        sqlx::query_scalar("SELECT count(*) FROM eval.bundles WHERE content_sha256 = 'reset-demo'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(leftover_bundles, 0, "reset must drop eval schema rows");
    let leftover_namespaces: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runtime.namespaces WHERE durable_key = 'reset-demo'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        leftover_namespaces, 0,
        "reset must drop runtime schema rows"
    );
    Ok(())
}

#[tokio::test]
async fn reset_local_drops_eval_and_runtime_schemas() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_engine reset_local test");
        return Ok(());
    };

    let handle = test_db.acquire_isolated().await?;
    let pool = connect(&handle.database_url).await?;

    sqlx::query("CREATE TABLE eval.reset_sentinel (id bigint)")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE TABLE runtime.reset_sentinel (id bigint)")
        .execute(&pool)
        .await?;

    mike::resync::reset_local(&pool).await?;

    let eval_sentinel: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('eval.reset_sentinel')::text")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        eval_sentinel, None,
        "reset must drop the eval schema, not only the app schemas it lists"
    );
    let runtime_sentinel: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('runtime.reset_sentinel')::text")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        runtime_sentinel, None,
        "reset must drop the runtime schema, not only the app schemas it lists"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
