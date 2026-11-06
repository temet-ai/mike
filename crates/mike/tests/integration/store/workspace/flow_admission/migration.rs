use chrono::{DateTime, Utc};
use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::test_support::TestDb;

const BASELINE_MIGRATION: &str = include_str!(
    "../../../../../src/store/persistence/migrations/20260910000001_session_flow_capture.sql"
);
const CAPTURE_EVIDENCE_MIGRATION: &str = include_str!(
    "../../../../../src/store/persistence/migrations/20260918000001_flow_admission_capture_derived_evidence.sql"
);

struct FlowColumnFixture<'a> {
    capture_version: Option<i32>,
    admitted_at: bool,
    producer: Option<&'a str>,
    hook_event_name: Option<&'a str>,
    source: Option<&'a str>,
}

async fn set_flow_columns(
    pool: &sqlx::PgPool,
    id: i64,
    fixture: FlowColumnFixture<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE sessions.sessions
            SET flow_capture_version = $2,
                flow_admitted_at = CASE WHEN $3 THEN clock_timestamp() ELSE NULL END,
                flow_producer = $4,
                flow_hook_event_name = $5,
                flow_start_source = $6
          WHERE id = $1",
    )
    .bind(id)
    .bind(fixture.capture_version)
    .bind(fixture.admitted_at)
    .bind(fixture.producer)
    .bind(fixture.hook_event_name)
    .bind(fixture.source)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn migrations_reapply_cleanly_and_the_constraint_accepts_only_known_combinations()
-> anyhow::Result<()> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    sqlx::raw_sql(BASELINE_MIGRATION)
        .execute(handle.pool())
        .await?;
    sqlx::raw_sql(CAPTURE_EVIDENCE_MIGRATION)
        .execute(handle.pool())
        .await?;

    let session = Uuid::new_v4();
    let id = mike::store::persistence::id_resolvers::resolve_session_id(
        handle.pool(),
        &session.to_string(),
        None,
        None,
    )
    .await?;

    for (capture_version, admitted_at, producer, hook_event_name, source) in [
        (
            Some(1),
            false,
            Some("mike"),
            Some("capture"),
            Some("capture"),
        ),
        (
            Some(0),
            true,
            Some("mike"),
            Some("capture"),
            Some("capture"),
        ),
        (
            Some(65536),
            true,
            Some("mike"),
            Some("capture"),
            Some("capture"),
        ),
        (Some(1), true, Some("mike"), Some("capture"), Some("other")),
        (
            Some(1),
            true,
            Some("other"),
            Some("capture"),
            Some("capture"),
        ),
        (Some(1), true, None, Some("capture"), Some("capture")),
        (
            Some(1),
            true,
            Some("claude_code"),
            Some("SessionStart"),
            Some("resume"),
        ),
    ] {
        let error = set_flow_columns(
            handle.pool(),
            id,
            FlowColumnFixture {
                capture_version,
                admitted_at,
                producer,
                hook_event_name,
                source,
            },
        )
        .await;
        assert!(
            error.is_err(),
            "combination {capture_version:?}/{producer:?}/{hook_event_name:?}/{source:?} \
             must be rejected"
        );
    }

    for (producer, hook_event_name, source) in [
        ("mike", "capture", "capture"),
        ("mike", "capture", "backfill"),
        ("claude_code", "SessionStart", "startup"),
    ] {
        set_flow_columns(
            handle.pool(),
            id,
            FlowColumnFixture {
                capture_version: Some(1),
                admitted_at: true,
                producer: Some(producer),
                hook_event_name: Some(hook_event_name),
                source: Some(source),
            },
        )
        .await?;
    }

    handle.pool().close().await;
    db.cleanup().await
}

#[tokio::test]
async fn migration_previous_session_shape_preserves_seed_and_no_enrollment() -> anyhow::Result<()> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut tx = handle.pool().begin().await?;
    sqlx::raw_sql(
        "DROP TRIGGER trg_sessions_row_changed ON sessions.sessions;
        ALTER TABLE sessions.sessions
        DROP COLUMN flow_capture_version, DROP COLUMN flow_admitted_at,
        DROP COLUMN flow_producer, DROP COLUMN flow_hook_event_name,
        DROP COLUMN flow_start_source",
    )
    .execute(&mut *tx)
    .await?;
    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind("/tmp/pre-flow-admission-project")
    .fetch_one(&mut *tx)
    .await?;
    let session = Uuid::new_v4();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen, title)
         VALUES ($1, $2, NOW(), NOW(), 'retained prior session') RETURNING id",
    )
    .bind(session)
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::raw_sql(BASELINE_MIGRATION).execute(&mut *tx).await?;
    let retained: (String, Option<i32>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT title, flow_capture_version, flow_admitted_at FROM sessions.sessions WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(retained, ("retained prior session".into(), None, None));
    tx.rollback().await?;
    handle.pool().close().await;
    db.cleanup().await
}
