use crate::test_db;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

use mike::memory_harvester::{
    HarvestContext, HarvesterConfig, harvest_from_response, scan_update_tags,
};
use mike::recall_injector::formatter::format_topic_catalog;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::harvest_runs::NewHarvestRun;

#[test]
fn update_tags_parsed_correctly_single_tag() {
    let text = r#"<mem:update topic="proxy-port">Use :9988.</mem:update>"#;
    let tags = scan_update_tags(text);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].topic, "proxy-port");
    assert_eq!(tags[0].kind, "convention");
    assert_eq!(tags[0].text, "Use :9988.");
}

#[test]
fn update_tags_parsed_correctly_explicit_kind() {
    let text = r#"<mem:update topic="danger" kind="warning">Don't do X.</mem:update>"#;
    let tags = scan_update_tags(text);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].kind, "warning");
}

#[test]
fn update_tags_parsed_correctly_multiple_tags() {
    let text = concat!(
        r#"<mem:update topic="a">First.</mem:update>"#,
        " gap ",
        r#"<mem:update topic="b">Second.</mem:update>"#,
    );
    let tags = scan_update_tags(text);
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].topic, "a");
    assert_eq!(tags[1].topic, "b");
}

#[test]
fn update_tags_skips_tag_without_topic() {
    let text = r#"<mem:update kind="warning">No topic.</mem:update>"#;
    assert!(scan_update_tags(text).is_empty());
}

#[test]
fn update_tags_skips_empty_body() {
    let text = r#"<mem:update topic="x"></mem:update>"#;
    assert!(scan_update_tags(text).is_empty());
}

async fn make_pool_and_persist(database_url: &str) -> anyhow::Result<(PgPool, PersistHandle)> {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await?;
    let persist = PersistHandle::connect(database_url).await?;
    Ok((pool, persist))
}

async fn ensure_project(pool: &PgPool) -> anyhow::Result<i64> {
    let project_path = format!("/test/memory-harvester-{}", uuid::Uuid::new_v4());
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects
             (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(&project_path)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn seed_request(pool: &PgPool, project_id: i64) -> anyhow::Result<i64> {
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions
             (session_uuid, project_id, first_seen, last_seen)
         VALUES (gen_random_uuid(), $1, NOW(), NOW())
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    let request_id: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
             (session_id, project_id, method_id, path, is_streaming,
              message_count, is_tool_loop, status_id, params,
              last_user_message, human_prompt, turn_type_id, turn_origin_id)
         SELECT $1, $2, m.id, '/v1/messages', false,
                1, false, s.id, '{}'::jsonb,
                'test', 'test', tt.id, orig.id
           FROM requests.methods m,
                requests.statuses s,
                requests.turn_types tt,
                requests.turn_origins orig
          WHERE m.code = 'POST'
            AND s.code = 'complete'
            AND tt.code = 'user_prompt'
            AND orig.code = 'primary'
         RETURNING id",
    )
    .bind(session_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    Ok(request_id)
}

#[test]
fn harvest_default_is_enabled() {
    let cfg = HarvesterConfig::default();
    assert!(cfg.enabled, "harvester must default to enabled");
}

async fn assert_harvest_writes_lesson_with_harvester_source(
    pool: &PgPool,
    persist: &PersistHandle,
) -> anyhow::Result<()> {
    let project_id = ensure_project(pool).await?;
    let request_id = seed_request(pool, project_id).await?;

    let text = r#"Some response. <mem:update topic="db-port">Use :6666.</mem:update> Done."#;
    let ctx = HarvestContext {
        project_id,
        session_id: None,
        request_id: Some(request_id),
        source_turn_id: None,
    };
    let cfg = HarvesterConfig {
        enabled: true,
        max_lesson_bytes: 4096,
    };

    let result = harvest_from_response(text, &ctx, persist, &cfg).await;

    assert_eq!(result.tags_found, 1);
    assert_eq!(result.lessons_written, 1);
    assert_eq!(result.lessons_skipped, 0);
    assert_eq!(result.lessons_truncated, 0);

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT lsrc.code, l.topic
           FROM memory.lessons l
           JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
          WHERE l.project_id = $1
            AND l.archived_at IS NULL
         ORDER BY l.created_at DESC
         LIMIT 1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;

    assert_eq!(row.0, "harvester");
    assert_eq!(row.1.as_deref(), Some("db-port"));
    Ok(())
}

async fn assert_harvest_dedup_on_conflict_does_nothing(
    pool: &PgPool,
    persist: &PersistHandle,
) -> anyhow::Result<()> {
    let project_id = ensure_project(pool).await?;
    let request_id = seed_request(pool, project_id).await?;

    let text = r#"<mem:update topic="key">Value.</mem:update>"#;
    let ctx = HarvestContext {
        project_id,
        session_id: None,
        request_id: Some(request_id),
        source_turn_id: None,
    };
    let cfg = HarvesterConfig {
        enabled: true,
        max_lesson_bytes: 4096,
    };

    let first = harvest_from_response(text, &ctx, persist, &cfg).await;
    assert_eq!(first.lessons_written, 1);
    assert_eq!(first.lessons_skipped, 0);

    let second = harvest_from_response(text, &ctx, persist, &cfg).await;
    assert_eq!(
        second.lessons_written, 0,
        "dedup: second call must not write"
    );
    assert_eq!(second.lessons_skipped, 1, "dedup: second call must skip");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory.lessons WHERE project_id = $1 AND archived_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(count, 1);
    Ok(())
}

async fn assert_harvest_provenance_required_both_none_returns_empty(
    pool: &PgPool,
    persist: &PersistHandle,
) -> anyhow::Result<()> {
    let project_id = ensure_project(pool).await?;

    let text = r#"<mem:update topic="x">Body.</mem:update>"#;
    let ctx = HarvestContext {
        project_id,
        session_id: None,
        request_id: None,
        source_turn_id: None,
    };
    let cfg = HarvesterConfig {
        enabled: true,
        max_lesson_bytes: 4096,
    };

    let result = harvest_from_response(text, &ctx, persist, &cfg).await;

    assert_eq!(
        result.tags_found, 0,
        "provenance guard fires before parsing"
    );
    assert_eq!(result.lessons_written, 0);
    assert_eq!(result.lessons_skipped, 0);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory.lessons WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    assert_eq!(count, 0);
    Ok(())
}

async fn assert_harvest_max_lesson_bytes_truncated(
    pool: &PgPool,
    persist: &PersistHandle,
) -> anyhow::Result<()> {
    let project_id = ensure_project(pool).await?;
    let request_id = seed_request(pool, project_id).await?;

    let long_body = "x".repeat(20);
    let text = format!(r#"<mem:update topic="long">{long_body}</mem:update>"#);
    let ctx = HarvestContext {
        project_id,
        session_id: None,
        request_id: Some(request_id),
        source_turn_id: None,
    };
    let cfg = HarvesterConfig {
        enabled: true,
        max_lesson_bytes: 10,
    };

    let result = harvest_from_response(&text, &ctx, persist, &cfg).await;

    assert_eq!(result.tags_found, 1);
    assert_eq!(result.lessons_written, 1);
    assert_eq!(result.lessons_truncated, 1, "truncation flag must be set");

    let stored_text: String =
        sqlx::query_scalar("SELECT text FROM memory.lessons WHERE project_id = $1 LIMIT 1")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    assert!(
        stored_text.len() <= cfg.max_lesson_bytes,
        "stored text ({} bytes) must not exceed max_lesson_bytes ({})",
        stored_text.len(),
        cfg.max_lesson_bytes
    );
    Ok(())
}

fn taught_update_example() -> anyhow::Result<String> {
    let catalog = format_topic_catalog(&["some-topic".to_string()])
        .ok_or_else(|| anyhow::anyhow!("catalog was empty"))?;
    let start = catalog
        .find("<mem:update")
        .ok_or_else(|| anyhow::anyhow!("catalog does not teach <mem:update>: {catalog}"))?;
    let close = "</mem:update>";
    let end = catalog[start..]
        .find(close)
        .ok_or_else(|| anyhow::anyhow!("catalog example is not closed: {catalog}"))?;
    Ok(catalog[start..start + end + close.len()].to_string())
}

async fn harvest_one(
    pool: &PgPool,
    persist: &PersistHandle,
    text: &str,
) -> anyhow::Result<(i64, mike::memory_harvester::HarvestResult)> {
    let project_id = ensure_project(pool).await?;
    let request_id = seed_request(pool, project_id).await?;
    let ctx = HarvestContext {
        project_id,
        session_id: None,
        request_id: Some(request_id),
        source_turn_id: None,
    };
    let cfg = HarvesterConfig::default();
    Ok((
        project_id,
        harvest_from_response(text, &ctx, persist, &cfg).await,
    ))
}

async fn assert_taught_example_is_harvested_verbatim(
    pool: &PgPool,
    persist: &PersistHandle,
) -> anyhow::Result<()> {
    let example = taught_update_example()?;
    let reply = format!("Noted for later. {example} Carrying on.");
    let (project_id, result) = harvest_one(pool, persist, &reply).await?;

    assert_eq!(
        result.tags_found, 1,
        "taught form not recognised: {example}"
    );
    assert_eq!(
        result.lessons_written, 1,
        "taught form not stored: {example}"
    );

    let row: (String, Option<String>, String) = sqlx::query_as(
        "SELECT lk.code, l.topic, l.text
           FROM memory.lessons l
           JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
          WHERE l.project_id = $1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(row.0, "convention");
    assert_eq!(row.1.as_deref(), Some("X"));
    assert_eq!(row.2, "the rule");
    Ok(())
}

async fn assert_taught_example_with_each_kind_is_harvested(
    pool: &PgPool,
    persist: &PersistHandle,
) -> anyhow::Result<()> {
    let example = taught_update_example()?;
    for kind in ["correction", "convention", "warning", "preference"] {
        let filled = example
            .replace("correction|convention|warning|preference", kind)
            .replace("the rule", &format!("rule for {kind}"));
        let (project_id, result) = harvest_one(pool, persist, &filled).await?;
        assert_eq!(
            result.lessons_written, 1,
            "kind {kind} not stored: {filled}"
        );
        let stored: String = sqlx::query_scalar(
            "SELECT lk.code
               FROM memory.lessons l
               JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
              WHERE l.project_id = $1",
        )
        .bind(project_id)
        .fetch_one(pool)
        .await?;
        assert_eq!(stored, kind);
    }
    Ok(())
}

async fn assert_harvest_run_is_recorded(
    pool: &PgPool,
    persist: &PersistHandle,
) -> anyhow::Result<()> {
    let project_id = ensure_project(pool).await?;
    let request_id = seed_request(pool, project_id).await?;
    persist
        .record_harvest_run(NewHarvestRun {
            project_id,
            session_id: None,
            request_id,
            tags_found: 3,
            lessons_written: 2,
            lessons_skipped: 1,
            lessons_truncated: 0,
        })
        .await?;
    let row: (i32, i32, i32, i32) = sqlx::query_as(
        "SELECT tags_found, lessons_written, lessons_skipped, lessons_truncated
           FROM memory.harvest_runs
          WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(row, (3, 2, 1, 0));
    Ok(())
}

#[tokio::test]
async fn harvest_covers_write_dedup_provenance_guard_and_truncation() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping harvest_covers_write_dedup_provenance_guard_and_truncation"
        );
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let (pool, persist) = make_pool_and_persist(&test_db.database_url).await?;

    assert_harvest_writes_lesson_with_harvester_source(&pool, &persist).await?;
    assert_harvest_dedup_on_conflict_does_nothing(&pool, &persist).await?;
    assert_harvest_provenance_required_both_none_returns_empty(&pool, &persist).await?;
    assert_harvest_max_lesson_bytes_truncated(&pool, &persist).await?;
    assert_taught_example_is_harvested_verbatim(&pool, &persist).await?;
    assert_taught_example_with_each_kind_is_harvested(&pool, &persist).await?;
    assert_harvest_run_is_recorded(&pool, &persist).await?;

    test_db.cleanup().await?;
    Ok(())
}
