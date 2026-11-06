use super::fixtures::{
    count_instances, get_request_instance, seed_dispatch, seed_session, seed_tool_use,
};

use crate::seed;
use crate::test_db;

use chrono::{TimeZone, Utc};
use seed::{SeedRequest, seed_project};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

pub(super) async fn create_instances_for_dispatches(
    pool: &PgPool,
    session_pk: i64,
) -> anyhow::Result<()> {
    let dispatches: Vec<(i64, Option<i64>, Option<String>)> = sqlx::query_as(
        "WITH sub AS (
             SELECT rr.id, rr.subagent_persona,
                    ROW_NUMBER() OVER (ORDER BY rr.id ASC) - 1 AS pos
               FROM requests.requests rr
               JOIN requests.turn_origins tor_sa
                 ON tor_sa.id = rr.turn_origin_id
                AND tor_sa.code = 'subagent'
              WHERE rr.session_id = $1
                AND rr.agent_instance_id IS NULL
         ),
         disp AS (
             SELECT ad.id AS dispatch_id, ad.agent_id, tu.tool_name,
                    ROW_NUMBER() OVER (ORDER BY ad.id ASC) - 1 AS pos
               FROM agents.dispatches ad
               JOIN tools.uses tu ON tu.id = ad.tool_use_id
               JOIN requests.requests r2 ON r2.id = tu.request_id
              WHERE r2.session_id = $1
         )
         SELECT d.dispatch_id, d.agent_id, sub.subagent_persona
           FROM disp d
           LEFT JOIN sub ON sub.pos = d.pos
           LEFT JOIN agents.instances ai_existing
             ON ai_existing.dispatch_id = d.dispatch_id
          WHERE d.tool_name = 'Agent'
            AND ai_existing.id IS NULL",
    )
    .bind(session_pk)
    .fetch_all(pool)
    .await?;

    for (dispatch_id, agent_id, subagent_persona) in dispatches {
        let instance_id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO agents.instances
                (session_id, dispatch_id, agent_id, subagent_persona)
             SELECT $1, $2, $3, $4
             WHERE NOT EXISTS (
                 SELECT 1 FROM agents.instances WHERE dispatch_id = $2
             )
             RETURNING id",
        )
        .bind(session_pk)
        .bind(dispatch_id)
        .bind(agent_id)
        .bind(&subagent_persona)
        .fetch_optional(pool)
        .await?;

        let Some(instance_id) = instance_id else {
            continue;
        };

        if let Some(persona) = subagent_persona.as_deref() {
            let first_req_id: Option<i64> = sqlx::query_scalar(
                "UPDATE requests.requests r
                    SET agent_instance_id = $2
                  FROM (
                      SELECT rr.id FROM requests.requests rr
                       WHERE rr.session_id       = $1
                         AND rr.turn_origin_id   = (SELECT id FROM requests.turn_origins WHERE code = 'subagent')
                         AND rr.subagent_persona = $3
                         AND rr.agent_instance_id IS NULL
                       ORDER BY rr.id ASC
                       LIMIT 1
                  ) target
                  WHERE r.id = target.id
                  RETURNING r.id",
            )
            .bind(session_pk)
            .bind(instance_id)
            .bind(persona)
            .fetch_optional(pool)
            .await?;

            if let Some(first_id) = first_req_id {
                sqlx::query(
                    "UPDATE agents.instances
                        SET first_request_id = $2,
                            started_at = (
                                SELECT created_at FROM requests.requests
                                 WHERE id = $2
                            )
                      WHERE id = $1
                        AND first_request_id IS NULL",
                )
                .bind(instance_id)
                .bind(first_id)
                .execute(pool)
                .await?;
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn single_dispatch_creates_one_instance() -> anyhow::Result<()> {
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
    let project_id = seed_project(&pool, "inst/single-dispatch").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 0, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);

    SeedRequest::new(1001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tool_use_pk = seed_tool_use(&pool, "tu-001", 1001, 0, t0).await?;
    let dispatch_id = seed_dispatch(&pool, tool_use_pk).await?;

    SeedRequest::new(1002, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("my-agent"))
        .created_at(t1)
        .insert(&pool)
        .await?;

    create_instances_for_dispatches(&pool, session_pk).await?;

    assert_eq!(count_instances(&pool, session_pk).await?, 1);

    let instance_dispatch: i64 =
        sqlx::query_scalar("SELECT dispatch_id FROM agents.instances WHERE session_id = $1")
            .bind(session_pk)
            .fetch_one(&pool)
            .await?;
    assert_eq!(instance_dispatch, dispatch_id);

    let req_instance = get_request_instance(&pool, 1002).await?;
    assert!(
        req_instance.is_some(),
        "sub-agent request 1002 should have agent_instance_id after back-fill"
    );

    let first_req: Option<Option<i64>> =
        sqlx::query_scalar("SELECT first_request_id FROM agents.instances WHERE session_id = $1")
            .bind(session_pk)
            .fetch_optional(&pool)
            .await?;
    assert_eq!(
        first_req.flatten(),
        Some(1002),
        "first_request_id must point at the first sub-agent request"
    );

    create_instances_for_dispatches(&pool, session_pk).await?;
    assert_eq!(count_instances(&pool, session_pk).await?, 1);

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn two_dispatches_create_two_instances() -> anyhow::Result<()> {
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
    let project_id = seed_project(&pool, "inst/two-dispatches").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 1, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);
    let t2 = t0 + chrono::Duration::seconds(2);

    SeedRequest::new(2001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tu_a = seed_tool_use(&pool, "tu-002a", 2001, 0, t0).await?;
    let tu_b = seed_tool_use(&pool, "tu-002b", 2001, 1, t0).await?;
    seed_dispatch(&pool, tu_a).await?;
    seed_dispatch(&pool, tu_b).await?;

    SeedRequest::new(2002, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("agent-a"))
        .created_at(t1)
        .insert(&pool)
        .await?;
    SeedRequest::new(2003, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("agent-b"))
        .created_at(t2)
        .insert(&pool)
        .await?;

    create_instances_for_dispatches(&pool, session_pk).await?;

    assert_eq!(
        count_instances(&pool, session_pk).await?,
        2,
        "two dispatches must produce two instance rows"
    );

    let inst_2002 = get_request_instance(&pool, 2002).await?;
    let inst_2003 = get_request_instance(&pool, 2003).await?;
    assert!(inst_2002.is_some(), "request 2002 should be bound");
    assert!(inst_2003.is_some(), "request 2003 should be bound");
    assert_ne!(
        inst_2002, inst_2003,
        "distinct personas must bind to distinct instances"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn duplicate_dispatch_insert_is_idempotent() -> anyhow::Result<()> {
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
    let project_id = seed_project(&pool, "inst/idempotent").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 2, 0).unwrap();
    let t1 = t0 + chrono::Duration::seconds(1);

    SeedRequest::new(3001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;
    let tu = seed_tool_use(&pool, "tu-003", 3001, 0, t0).await?;
    seed_dispatch(&pool, tu).await?;
    SeedRequest::new(3002, project_id)
        .session_pk(session_pk)
        .turn_origin("subagent")
        .subagent_persona(Some("idempotent-agent"))
        .created_at(t1)
        .insert(&pool)
        .await?;

    for _ in 0..3 {
        create_instances_for_dispatches(&pool, session_pk).await?;
    }
    assert_eq!(count_instances(&pool, session_pk).await?, 1);

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn session_with_no_dispatches_produces_no_instances() -> anyhow::Result<()> {
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
    let project_id = seed_project(&pool, "inst/no-dispatch").await?;
    let session_pk = seed_session(&pool, &session_id, project_id).await?;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 21, 10, 5, 0).unwrap();
    SeedRequest::new(6001, project_id)
        .session_pk(session_pk)
        .created_at(t0)
        .insert(&pool)
        .await?;

    create_instances_for_dispatches(&pool, session_pk).await?;

    assert_eq!(
        count_instances(&pool, session_pk).await?,
        0,
        "session with no dispatches must produce zero instance rows"
    );

    test_db.cleanup().await?;
    Ok(())
}
