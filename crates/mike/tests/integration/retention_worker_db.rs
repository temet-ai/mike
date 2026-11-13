use crate::test_db::TestDb;

use mike::config::ProxyConfig;
use mike::retention_worker::run_retention_sweep_once;
use mike::state::AppState;

async fn seed_session(
    pool: &sqlx::PgPool,
    project_id: i64,
    last_seen: chrono::DateTime<chrono::Utc>,
    pinned: bool,
) -> anyhow::Result<uuid::Uuid> {
    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions
            (session_uuid, project_id, first_seen, last_seen, pinned)
         VALUES ($1, $2, $3, $3, $4)",
        session_uuid,
        project_id,
        last_seen,
        pinned,
    )
    .execute(pool)
    .await?;
    Ok(session_uuid)
}

#[tokio::test]
async fn sweep_tick_removes_expired_unpinned_keeps_pinned_and_unexpired() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;

    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "https://api.anthropic.com",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &db.database_url,
        "--default-ttl",
        "1h",
    ])?;
    let state = AppState::new(config).await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    let project_id: i64 = sqlx::query_scalar!(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ('/tmp/retention-worker-test', '/tmp/retention-worker-test', NOW(), NOW())
         RETURNING id"
    )
    .fetch_one(&pool)
    .await?;

    let now = chrono::Utc::now();
    let expired_unpinned =
        seed_session(&pool, project_id, now - chrono::Duration::hours(2), false).await?;
    let pinned_but_old =
        seed_session(&pool, project_id, now - chrono::Duration::hours(2), true).await?;
    let unexpired = seed_session(&pool, project_id, now, false).await?;

    let outcome = run_retention_sweep_once(&state).await?;
    assert_eq!(
        outcome.sessions_removed, 1,
        "exactly the expired, unpinned session must be swept"
    );

    let remaining: Vec<uuid::Uuid> =
        sqlx::query_scalar!("SELECT session_uuid FROM sessions.sessions")
            .fetch_all(&pool)
            .await?;
    assert!(
        !remaining.contains(&expired_unpinned),
        "expired unpinned session must be gone"
    );
    assert!(
        remaining.contains(&pinned_but_old),
        "pinned session must survive despite being older than the TTL"
    );
    assert!(
        remaining.contains(&unexpired),
        "unexpired session must survive"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

fn parse_metric_value(rendered: &str, name: &str) -> Option<f64> {
    rendered.lines().find_map(|line| {
        let rest = line.strip_prefix(name)?;
        let rest = rest.strip_prefix(' ')?;
        rest.trim().parse::<f64>().ok()
    })
}

#[tokio::test]
async fn sweep_tick_emits_observable_metrics_even_with_zero_removals() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;

    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "https://api.anthropic.com",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &db.database_url,
        "--default-ttl",
        "1h",
    ])?;
    let state = AppState::new(config).await?;

    let outcome = run_retention_sweep_once(&state).await?;
    assert_eq!(outcome.sessions_removed, 0);
    assert_eq!(outcome.messages_removed, 0);

    let rendered = state.metrics_handle.render();
    let ticks = parse_metric_value(&rendered, "retention_sweep_ticks_total").ok_or_else(|| {
        anyhow::anyhow!("retention_sweep_ticks_total missing from the metrics render")
    })?;
    assert!(
        ticks >= 1.0,
        "a zero-removal tick must still increment the ticks counter, got {ticks}"
    );

    let last_run = parse_metric_value(&rendered, "retention_sweep_last_run_unix_seconds")
        .ok_or_else(|| {
            anyhow::anyhow!("retention_sweep_last_run_unix_seconds missing from the metrics render")
        })?;
    let now = chrono::Utc::now().timestamp() as f64;
    assert!(
        (now - last_run).abs() < 30.0,
        "last-run gauge must reflect this tick: got {last_run}, now {now}"
    );

    db.cleanup().await?;
    Ok(())
}
