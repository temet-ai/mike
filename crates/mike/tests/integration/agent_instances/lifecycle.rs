use super::creation::create_instances_for_dispatches;
use super::fixtures::{
    count_instances, get_request_instance, seed_dispatch, seed_session, seed_tool_use,
};

use crate::seed;
use crate::test_db;

use chrono::{TimeZone, Utc};
use seed::{SeedRequest, seed_project};
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn agent_instances_cascade_delete_with_session() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let project_id = seed_project(&pool, "inst/cascade-del").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 6, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);
    SeedRequest::new(7001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tu = seed_tool_use(&pool, "tu-007", 7001, 0, t0).await?;
    seed_dispatch(&pool, tu).await?;
    SeedRequest::new(7002, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("cascade-agent"))
        .created_at(t1)
        .insert(&pool)
        .await?;

    create_instances_for_dispatches(&pool, session_pk).await?;
    assert_eq!(count_instances(&pool, session_pk).await?, 1);

    sqlx::query("DELETE FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
        .bind(session_id)
        .execute(&pool)
        .await?;

    assert_eq!(
        count_instances(&pool, session_pk).await?,
        0,
        "agent_instances must cascade-delete when the session is removed"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn agent_instance_fk_set_null_on_instance_delete() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let project_id = seed_project(&pool, "inst/set-null").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 7, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);
    SeedRequest::new(8001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tu = seed_tool_use(&pool, "tu-008", 8001, 0, t0).await?;
    seed_dispatch(&pool, tu).await?;
    SeedRequest::new(8002, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("set-null-agent"))
        .created_at(t1)
        .insert(&pool)
        .await?;

    create_instances_for_dispatches(&pool, session_pk).await?;

    let instance_id = get_request_instance(&pool, 8002)
        .await?
        .ok_or_else(|| anyhow::anyhow!("8002 should be bound after back-fill"))?;

    sqlx::query("DELETE FROM agents.instances WHERE id = $1")
        .bind(instance_id)
        .execute(&pool)
        .await?;

    let after = get_request_instance(&pool, 8002).await?;
    assert_eq!(
        after, None,
        "agent_instance_id must be SET NULL after the instance row is deleted"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn instance_agent_id_resolves_from_dispatch_not_persona() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let project_id = seed_project(&pool, "inst/dispatch-not-persona").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 8, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);

    SeedRequest::new(9001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tool_use_pk = seed_tool_use(&pool, "tu-009", 9001, 0, t0).await?;
    let dispatch_id = seed_dispatch(&pool, tool_use_pk).await?;
    let dispatch_agent_id: i64 =
        sqlx::query_scalar("SELECT agent_id FROM agents.dispatches WHERE id = $1")
            .bind(dispatch_id)
            .fetch_one(&pool)
            .await?;

    SeedRequest::new(9002, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("a completely different persona sentence"))
        .created_at(t1)
        .insert(&pool)
        .await?;

    create_instances_for_dispatches(&pool, session_pk).await?;

    let instance_agent_id: Option<i64> =
        sqlx::query_scalar("SELECT agent_id FROM agents.instances WHERE dispatch_id = $1")
            .bind(dispatch_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        instance_agent_id,
        Some(dispatch_agent_id),
        "instance agent_id must equal the dispatch's own agent_id, not a persona-name lookup"
    );

    let persona_row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agents.agents WHERE name = 'a completely different persona sentence'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        persona_row_count, 0,
        "a persona sentence must never mint its own agents.agents row"
    );

    test_db.cleanup().await?;
    Ok(())
}
