use crate::test_db;

use anyhow::Context as _;
use figment::providers::Format as _;
use mike::config::ProxyConfig;
use mike::state::AppState;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use test_db::TestDb;

use super::support::{insert_lesson, ok_response};

async fn run_gc_scenario(state: &AppState, pool: &PgPool) -> anyhow::Result<()> {
    let project_path = "gc-proj";
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;

    let expired_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, expires_at)
         SELECT $1, lk.id, 'stale', ls.id, lsrc.id, NOW() - INTERVAL '1 hour'
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'warning' AND ls.code = 'daily' AND lsrc.code = 'operator'
         RETURNING uuid",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    let live_id = insert_lesson(pool, project_id, "warning", "fresh", "daily").await?;

    mike::lesson_worker::gc_once(state).await;

    let expired_state: Option<(String, Option<String>, bool)> = sqlx::query_as(
        "SELECT ls.code, lar.code, l.archived_at IS NOT NULL
           FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           LEFT JOIN memory.lesson_archived_reasons lar ON lar.id = l.archived_reason_id
          WHERE l.uuid = $1",
    )
    .bind(expired_id)
    .fetch_optional(pool)
    .await?;
    let (stage, reason, has_archived_at) =
        expired_state.context("expired lesson row must still exist after GC")?;
    assert_eq!(stage, "archived", "expired lesson must be archived by GC");
    assert_eq!(
        reason.as_deref(),
        Some("expired"),
        "archive reason must record why the lesson was retired"
    );
    assert!(has_archived_at, "archived_at must be stamped");

    let live_state: Option<(String, bool)> = sqlx::query_as(
        "SELECT ls.code, l.archived_at IS NOT NULL
           FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.uuid = $1",
    )
    .bind(live_id)
    .fetch_optional(pool)
    .await?;
    let (live_stage, live_archived) = live_state.context("non-expired lesson must survive GC")?;
    assert_ne!(live_stage, "archived", "a live lesson must stay unarchived");
    assert!(!live_archived, "a live lesson must not be stamped archived");
    Ok(())
}

async fn run_trim_scenario(state: &AppState, pool: &PgPool) -> anyhow::Result<()> {
    let project_path = "trim-proj";
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;

    for i in 0..4 {
        insert_lesson(
            pool,
            project_id,
            "convention",
            &format!("lesson {i}"),
            "daily",
        )
        .await?;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    mike::lesson_worker::nightly_once(state).await;

    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
          WHERE l.project_id = $1 AND ls.code != 'archived'",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        active, 2,
        "trim must cap active lessons at max_lessons_per_project"
    );

    let archived: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory.lessons l
           JOIN memory.lesson_stages ls ON ls.id = l.stage_id
           JOIN memory.lesson_archived_reasons ar ON ar.id = l.archived_reason_id
          WHERE l.project_id = $1 AND ls.code = 'archived' AND ar.code = 'trimmed'",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        archived, 2,
        "two oldest excess lessons must be archived as 'trimmed'"
    );
    Ok(())
}

async fn boot_state_with_trim_cap(
    database_url: &str,
    upstream_url: &str,
) -> anyhow::Result<AppState> {
    let tmp = tempfile::TempDir::new()?;
    let toml_path = tmp.path().join("config.toml");
    std::fs::write(
        &toml_path,
        format!(
            "persist_enabled = true\ndatabase_url = \"{database_url}\"\n\n[lesson_worker]\nenabled = false\nmax_lessons_per_project = 2\n"
        ),
    )?;
    mike::feature_settings::install();
    let base = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Toml::file(&toml_path));
    let mut config: ProxyConfig = base
        .extract()
        .map_err(|e| anyhow::anyhow!("retention config extract: {e}"))?;
    config.proxy_listen_addr = "127.0.0.1:0".to_string();
    config.inspect_listen_addr = "127.0.0.1:0".to_string();
    config.upstream_url = upstream_url.to_string();
    let state = AppState::new(config).await?;
    mike::lesson_delivery::hooks::install(&state);
    Ok(state)
}

#[tokio::test]
async fn lesson_gc_and_trim_archive_expired_and_excess_rows() -> anyhow::Result<()> {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .mount(&mock)
        .await;

    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let state = boot_state_with_trim_cap(&db.database_url, &mock.uri()).await?;
    let shutdown = state.shutdown();

    run_gc_scenario(&state, &pool).await?;
    run_trim_scenario(&state, &pool).await?;

    shutdown.cancel();
    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
