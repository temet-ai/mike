use crate::seed;
use crate::test_db;

use anyhow::Context;
use chrono::{DateTime, Utc};
use seed::{seed_project, seed_session};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;
use uuid::Uuid;

async fn set_session_overview(
    pool: &PgPool,
    session_uuid: Uuid,
    overview: &str,
    generated_at: Option<DateTime<Utc>>,
) -> anyhow::Result<bool> {
    let result = sqlx::query!(
        "UPDATE sessions.sessions
            SET overview = $2,
                overview_generated_at = $3
          WHERE session_uuid = $1",
        session_uuid,
        overview,
        generated_at,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn set_session_overview_generated(
    pool: &PgPool,
    session_uuid: Uuid,
    overview: &str,
    generated_at: DateTime<Utc>,
) -> anyhow::Result<bool> {
    let result = sqlx::query!(
        "UPDATE sessions.sessions
            SET overview = $2,
                overview_generated_at = $3
          WHERE session_uuid = $1
            AND (overview IS NULL OR overview_generated_at IS NOT NULL)",
        session_uuid,
        overview,
        generated_at,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn read_overview_columns(
    pool: &PgPool,
    session_uuid: Uuid,
) -> anyhow::Result<(Option<String>, Option<DateTime<Utc>>)> {
    let row = sqlx::query!(
        r#"SELECT overview AS "overview?", overview_generated_at AS "overview_generated_at?"
             FROM sessions.sessions WHERE session_uuid = $1"#,
        session_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok((row.overview, row.overview_generated_at))
}

async fn read_overview_via_summary_view(
    pool: &PgPool,
    session_uuid: Uuid,
) -> anyhow::Result<Option<(Option<String>, Option<DateTime<Utc>>)>> {
    let row = sqlx::query!(
        r#"SELECT overview AS "overview?", overview_generated_at AS "overview_generated_at?"
             FROM mike.session_summary_v
            WHERE session_id = $1"#,
        session_uuid.to_string(),
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.overview, r.overview_generated_at)))
}

#[tokio::test]
async fn set_overview_manual_override_leaves_generated_at_null() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id =
        seed_project(&pool, &format!("/proj/overview-manual-{}", Uuid::new_v4())).await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&pool, project_id, &session_uuid.to_string()).await?;

    let updated =
        set_session_overview(&pool, session_uuid, "Operator-authored summary.", None).await?;
    assert!(updated, "set against a known session must affect one row");

    let (overview, generated_at) = read_overview_columns(&pool, session_uuid).await?;
    assert_eq!(overview.as_deref(), Some("Operator-authored summary."));
    assert!(
        generated_at.is_none(),
        "manual override must leave overview_generated_at NULL"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_overview_llm_generated_stamps_timestamp() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("/proj/overview-generated-{}", Uuid::new_v4()),
    )
    .await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&pool, project_id, &session_uuid.to_string()).await?;

    let generated_at = Utc::now() - chrono::Duration::seconds(5);
    let generated_at = DateTime::<Utc>::from_timestamp_micros(generated_at.timestamp_micros())
        .context("valid microsecond timestamp")?;

    let updated = set_session_overview(
        &pool,
        session_uuid,
        "The session refactored the auth module and added tests.",
        Some(generated_at),
    )
    .await?;
    assert!(updated, "set against a known session must affect one row");

    let (overview, stored_generated_at) = read_overview_columns(&pool, session_uuid).await?;
    assert_eq!(
        overview.as_deref(),
        Some("The session refactored the auth module and added tests.")
    );
    assert_eq!(
        stored_generated_at,
        Some(generated_at),
        "LLM-generated set must stamp the exact generation timestamp"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_overview_unknown_session_affects_zero_rows() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let unknown = Uuid::new_v4();
    let updated = set_session_overview(&pool, unknown, "orphan overview", None).await?;
    assert!(
        !updated,
        "set against an unknown session must affect zero rows (handler -> 404)"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_summary_view_surfaces_overview_fields() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("/proj/overview-projection-{}", Uuid::new_v4()),
    )
    .await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&pool, project_id, &session_uuid.to_string()).await?;

    let generated_at = DateTime::<Utc>::from_timestamp_micros(Utc::now().timestamp_micros())
        .context("valid microsecond timestamp")?;
    assert!(
        set_session_overview(
            &pool,
            session_uuid,
            "Narrative overview surfaced through the view.",
            Some(generated_at),
        )
        .await?
    );

    let projected = read_overview_via_summary_view(&pool, session_uuid)
        .await?
        .context("session_summary_v must return a row for a seeded session")?;
    assert_eq!(
        projected.0.as_deref(),
        Some("Narrative overview surfaced through the view.")
    );
    assert_eq!(projected.1, Some(generated_at));

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_overview_generated_writes_when_overview_is_null() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("/proj/overview-guarded-null-{}", Uuid::new_v4()),
    )
    .await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&pool, project_id, &session_uuid.to_string()).await?;

    let generated_at = DateTime::<Utc>::from_timestamp_micros(Utc::now().timestamp_micros())
        .context("valid microsecond timestamp")?;
    let updated = set_session_overview_generated(
        &pool,
        session_uuid,
        "The judge wrote this overview from the session digest.",
        generated_at,
    )
    .await?;
    assert!(
        updated,
        "guarded setter must write when no overview exists yet"
    );

    let (overview, stored_generated_at) = read_overview_columns(&pool, session_uuid).await?;
    assert_eq!(
        overview.as_deref(),
        Some("The judge wrote this overview from the session digest.")
    );
    assert_eq!(stored_generated_at, Some(generated_at));

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_overview_generated_overwrites_prior_llm_generated_overview() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("/proj/overview-guarded-regenerate-{}", Uuid::new_v4()),
    )
    .await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&pool, project_id, &session_uuid.to_string()).await?;

    let first_pass = DateTime::<Utc>::from_timestamp_micros(
        (Utc::now() - chrono::Duration::minutes(10)).timestamp_micros(),
    )
    .context("valid microsecond timestamp")?;
    assert!(
        set_session_overview_generated(&pool, session_uuid, "First-pass overview.", first_pass)
            .await?
    );

    let second_pass = DateTime::<Utc>::from_timestamp_micros(Utc::now().timestamp_micros())
        .context("valid microsecond timestamp")?;
    let updated = set_session_overview_generated(
        &pool,
        session_uuid,
        "Second-pass overview after a rubric bump.",
        second_pass,
    )
    .await?;
    assert!(
        updated,
        "guarded setter must overwrite an existing LLM-generated overview"
    );

    let (overview, stored_generated_at) = read_overview_columns(&pool, session_uuid).await?;
    assert_eq!(
        overview.as_deref(),
        Some("Second-pass overview after a rubric bump.")
    );
    assert_eq!(stored_generated_at, Some(second_pass));

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn set_overview_generated_refuses_operator_authored_overview() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping DB test");
        return Ok(());
    };
    let handle = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("/proj/overview-guarded-operator-{}", Uuid::new_v4()),
    )
    .await?;
    let session_uuid = Uuid::new_v4();
    seed_session(&pool, project_id, &session_uuid.to_string()).await?;

    assert!(set_session_overview(&pool, session_uuid, "Operator wrote this by hand.", None).await?);

    let generated_at = DateTime::<Utc>::from_timestamp_micros(Utc::now().timestamp_micros())
        .context("valid microsecond timestamp")?;
    let updated = set_session_overview_generated(
        &pool,
        session_uuid,
        "The judge tried to overwrite the operator's note.",
        generated_at,
    )
    .await?;
    assert!(
        !updated,
        "guarded setter must refuse to overwrite an operator-authored overview"
    );

    let (overview, stored_generated_at) = read_overview_columns(&pool, session_uuid).await?;
    assert_eq!(
        overview.as_deref(),
        Some("Operator wrote this by hand."),
        "the operator's overview must survive the refused write"
    );
    assert!(
        stored_generated_at.is_none(),
        "the operator-authored NULL generated_at must be untouched"
    );

    handle.cleanup().await?;
    Ok(())
}
