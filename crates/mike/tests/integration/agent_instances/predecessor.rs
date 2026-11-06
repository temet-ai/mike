use super::creation::create_instances_for_dispatches;
use super::fixtures::{get_request_instance, seed_dispatch, seed_session, seed_tool_use};

use crate::seed;
use crate::test_db;

use chrono::{DateTime, TimeZone, Utc};
use seed::{SeedRequest, seed_project};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

async fn predecessor_primary_via_sql(
    pool: &PgPool,
    session_pk: i64,
    created_at: DateTime<Utc>,
) -> anyhow::Result<Option<i64>> {
    let row: Option<i64> = sqlx::query_scalar(
        "SELECT id
         FROM   requests.requests
         WHERE  session_id        = $1
           AND  turn_origin_id    IN (
               SELECT id FROM requests.turn_origins
                WHERE code IN ('primary', 'primary_first')
           )
           AND  agent_instance_id IS NULL
           AND  created_at        < $2
         ORDER  BY created_at DESC, id DESC
         LIMIT  1",
    )
    .bind(session_pk)
    .bind(created_at)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn predecessor_subagent_via_sql(
    pool: &PgPool,
    session_pk: i64,
    created_at: DateTime<Utc>,
    agent_instance_id: i64,
) -> anyhow::Result<Option<i64>> {
    let row: Option<i64> = sqlx::query_scalar(
        "SELECT id
         FROM   requests.requests
         WHERE  session_id        = $1
           AND  turn_origin_id    = (SELECT id FROM requests.turn_origins WHERE code = 'subagent')
           AND  agent_instance_id = $3
           AND  created_at        < $2
         ORDER  BY created_at DESC, id DESC
         LIMIT  1",
    )
    .bind(session_pk)
    .bind(created_at)
    .bind(agent_instance_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[tokio::test]
async fn predecessor_primary_scoped_to_null_instance() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let project_id = seed_project(&pool, "inst/pred-primary").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 3, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);
    let t2 = t0 + chrono::Duration::seconds(2);

    SeedRequest::new(4001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    SeedRequest::new(4002, project_id)
        .session_pk(session_pk)
        .created_at(t1)
        .insert(&pool)
        .await?;
    SeedRequest::new(4003, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("other"))
        .created_at(t2)
        .insert(&pool)
        .await?;

    let pred = predecessor_primary_via_sql(&pool, session_pk, t1).await?;
    assert_eq!(
        pred,
        Some(4001),
        "4002 at t1 must have predecessor 4001 at t0"
    );

    let pred_first = predecessor_primary_via_sql(&pool, session_pk, t0).await?;
    assert_eq!(
        pred_first, None,
        "4001 is the first primary — no predecessor"
    );

    let pred_after_subagent = predecessor_primary_via_sql(&pool, session_pk, t2).await?;
    assert_eq!(
        pred_after_subagent,
        Some(4002),
        "primary search after t2 should resolve 4002 (primary), not 4003 (subagent)"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn predecessor_subagent_scoped_to_same_instance() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let project_id = seed_project(&pool, "inst/pred-subagent").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 4, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);
    let t2 = t0 + chrono::Duration::seconds(2);

    SeedRequest::new(5001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tu = seed_tool_use(&pool, "tu-005", 5001, 0, t0).await?;
    seed_dispatch(&pool, tu).await?;

    SeedRequest::new(5002, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("sub-a"))
        .created_at(t1)
        .insert(&pool)
        .await?;
    SeedRequest::new(5003, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("sub-a"))
        .created_at(t2)
        .insert(&pool)
        .await?;

    create_instances_for_dispatches(&pool, session_pk).await?;

    let instance_id = get_request_instance(&pool, 5002)
        .await?
        .ok_or_else(|| anyhow::anyhow!("5002 should be bound after back-fill"))?;

    let pred = predecessor_subagent_via_sql(&pool, session_pk, t2, instance_id).await?;
    assert_eq!(
        pred,
        Some(5002),
        "5003 predecessor must be 5002 (same instance)"
    );

    let pred_first = predecessor_subagent_via_sql(&pool, session_pk, t1, instance_id).await?;
    assert_eq!(
        pred_first, None,
        "5002 is the first of its instance — no predecessor"
    );

    test_db.cleanup().await?;
    Ok(())
}
