use crate::test_db;

use mike::state::AppState;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use test_db::TestDb;

use super::support::{boot_proxy, insert_lesson, ok_response};

async fn run_snapshot_refresh_scenario(state: &AppState, pool: &PgPool) -> anyhow::Result<()> {
    let project_path = "snapshot-refresh-proj";
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;

    mike::lesson_worker::refresh_lesson_cache(state).await;
    assert!(
        state.injectable_lessons(project_path).is_empty(),
        "snapshot must be empty before any daily/long_term lesson exists"
    );

    insert_lesson(pool, project_id, "convention", "raw-only lesson", "raw").await?;
    mike::lesson_worker::refresh_lesson_cache(state).await;
    assert_eq!(
        state.injectable_lessons(project_path).len(),
        1,
        "explicit operator lessons are injectable before indexing"
    );
    let judge = insert_lesson(
        pool,
        project_id,
        "correction",
        "unapproved judge candidate",
        "raw",
    )
    .await?;
    sqlx::query("UPDATE memory.lessons SET source_id = src.id FROM memory.lesson_sources src WHERE uuid = $1 AND src.code = 'judge'")
        .bind(judge).execute(pool).await?;
    mike::lesson_worker::refresh_lesson_cache(state).await;
    assert_eq!(
        state.injectable_lessons(project_path).len(),
        1,
        "raw judge candidates remain excluded"
    );

    insert_lesson(
        pool,
        project_id,
        "warning",
        "daily lesson now live",
        "daily",
    )
    .await?;
    mike::lesson_worker::refresh_lesson_cache(state).await;
    let after = state.injectable_lessons(project_path);
    assert_eq!(
        after.len(),
        2,
        "operator raw and daily lessons must be injectable"
    );
    assert_eq!(after[0].text, "daily lesson now live");
    Ok(())
}

async fn run_staged_promotion_scenario(state: &AppState, pool: &PgPool) -> anyhow::Result<()> {
    let project_path = "promotion-proj";
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;

    let id = insert_lesson(pool, project_id, "correction", "promote me", "daily").await?;

    mike::lesson_worker::nightly_once(state).await;

    let stage: String = sqlx::query_scalar(
        "SELECT ls.code FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.uuid = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    assert_eq!(stage, "long_term", "daily lesson must promote to long_term");

    mike::lesson_worker::refresh_lesson_cache(state).await;
    let after = state.injectable_lessons(project_path);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].text, "promote me");
    Ok(())
}

#[tokio::test]
async fn snapshot_refresh_reflects_new_lesson_and_stages_promote_on_nightly() -> anyhow::Result<()>
{
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping snapshot_refresh_reflects_new_lesson_and_stages_promote_on_nightly"
        );
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let (state, _proxy_url, shutdown) = boot_proxy(&mock.uri(), &db.database_url).await?;

    run_snapshot_refresh_scenario(&state, &pool).await?;
    run_staged_promotion_scenario(&state, &pool).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
