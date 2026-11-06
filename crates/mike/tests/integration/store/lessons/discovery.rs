use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::test_support;

use crate::test_db::TestDb;

#[tokio::test]
async fn discover_idle_and_daily_projects_filter_by_stage() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let idle_proj =
        test_support::seed_project(&pool, &format!("w3-idle-{}", uuid::Uuid::new_v4())).await?;
    let daily_proj =
        test_support::seed_project(&pool, &format!("w3-daily-{}", uuid::Uuid::new_v4())).await?;
    let fresh_proj =
        test_support::seed_project(&pool, &format!("w3-fresh-{}", uuid::Uuid::new_v4())).await?;

    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, created_at)
         SELECT $1, lk.id, 'idle raw', ls.id, lsrc.id, NOW() - INTERVAL '2 hours'
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'warning' AND ls.code = 'raw' AND lsrc.code = 'operator'",
    )
    .bind(idle_proj)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, 'fresh raw', ls.id, lsrc.id
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'warning' AND ls.code = 'raw' AND lsrc.code = 'operator'",
    )
    .bind(fresh_proj)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, 'a daily', ls.id, lsrc.id
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'warning' AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(daily_proj)
    .execute(&pool)
    .await?;

    let idle = handle.discover_idle_projects(3600).await?;
    assert!(idle.contains(&idle_proj), "idle project must be discovered");
    assert!(
        !idle.contains(&fresh_proj),
        "a project whose raw lesson is younger than idle_secs must NOT be idle"
    );
    assert!(
        !idle.contains(&daily_proj),
        "a daily-only project has no raw/un-embedded lessons → not idle"
    );

    let daily = handle.discover_daily_projects().await?;
    assert!(
        daily.contains(&daily_proj),
        "daily project must be discovered"
    );
    assert!(
        !daily.contains(&idle_proj) && !daily.contains(&fresh_proj),
        "raw-only projects must NOT appear in discover_daily_projects"
    );

    Ok(())
}
