use crate::test_db;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

const MIGRATION_SQL: &str = include_str!(
    "../../../mike-base/src/store/persistence/migrations/20260721040000_heal_sentinel_session_projects.sql"
);

const SENTINEL_PROJECT_PATH: &str = "(no project)";

async fn insert_project(pool: &sqlx::PgPool, project_path: &str) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, now(), now())
         RETURNING id",
    )
    .bind(project_path)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn insert_session(pool: &sqlx::PgPool, project_id: i64) -> anyhow::Result<uuid::Uuid> {
    let session_uuid = uuid::Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
        session_uuid,
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(session_uuid)
}

async fn insert_request(
    pool: &sqlx::PgPool,
    session_uuid: uuid::Uuid,
    project_id: i64,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming,
             message_count, is_tool_loop, status_id,
             params, last_user_message, human_prompt, turn_type_id, turn_origin_id,
             created_at
         )
         SELECT ss.id, $2, rm.id, '/v1/messages', false,
                0, false, rs.id, '{}'::jsonb, '', '', rtt.id, rto.id, now()
           FROM sessions.sessions ss,
                requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE ss.session_uuid = $1
            AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
        session_uuid,
        project_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn session_project_id(pool: &sqlx::PgPool, session_uuid: uuid::Uuid) -> anyhow::Result<i64> {
    let id: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session_uuid)
            .fetch_one(pool)
            .await?;
    Ok(id)
}

#[tokio::test]
async fn heal_sentinel_session_projects_applies_cleanly_to_fresh_empty_db() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping heal_sentinel_session_projects fresh-db test"
        );
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must apply cleanly against a fresh, empty schema")?;

    let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions.sessions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(session_count, 0, "a fresh DB has no session rows to touch");

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn heal_sentinel_session_projects_heals_from_workspace_registration() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping heal_sentinel_session_projects registration test"
        );
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let unique = uuid::Uuid::new_v4();
    let sentinel_id = insert_project(&pool, SENTINEL_PROJECT_PATH).await?;
    let real_project_id = insert_project(&pool, &format!("real-project-{unique}")).await?;

    let dirty_session = insert_session(&pool, sentinel_id).await?;
    let dirty_session_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(dirty_session)
            .fetch_one(&pool)
            .await?;
    sqlx::query!(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, lease_expires_at)
         VALUES ($1, $2, '/tmp/dirty', now() + interval '15 minutes')",
        dirty_session_id,
        real_project_id,
    )
    .execute(&pool)
    .await?;

    let project_a_id = insert_project(&pool, &format!("project-a-{unique}")).await?;
    let project_b_id = insert_project(&pool, &format!("project-b-{unique}")).await?;
    let healthy_session = insert_session(&pool, project_a_id).await?;
    let healthy_session_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(healthy_session)
            .fetch_one(&pool)
            .await?;
    sqlx::query!(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, lease_expires_at)
         VALUES ($1, $2, '/tmp/healthy', now() + interval '15 minutes')",
        healthy_session_id,
        project_b_id,
    )
    .execute(&pool)
    .await?;

    sqlx::raw_sql(MIGRATION_SQL).execute(&pool).await?;

    assert_eq!(
        session_project_id(&pool, dirty_session).await?,
        real_project_id,
        "the sentinel session must heal onto its registration's real project"
    );
    assert_eq!(
        session_project_id(&pool, healthy_session).await?,
        project_a_id,
        "a session already on a real project must never be re-pointed by a registration"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn heal_sentinel_session_projects_heals_from_majority_request_project() -> anyhow::Result<()>
{
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping heal_sentinel_session_projects majority test"
        );
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let unique = uuid::Uuid::new_v4();
    let sentinel_id = insert_project(&pool, SENTINEL_PROJECT_PATH).await?;
    let majority_project_id = insert_project(&pool, &format!("majority-project-{unique}")).await?;
    let minority_project_id = insert_project(&pool, &format!("minority-project-{unique}")).await?;

    let session = insert_session(&pool, sentinel_id).await?;
    for _ in 0..3 {
        insert_request(&pool, session, majority_project_id).await?;
    }
    insert_request(&pool, session, minority_project_id).await?;

    sqlx::raw_sql(MIGRATION_SQL).execute(&pool).await?;

    assert_eq!(
        session_project_id(&pool, session).await?,
        majority_project_id,
        "the session must heal onto the project backing the most requests"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn heal_sentinel_session_projects_tiebreak_picks_lower_project_id() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping heal_sentinel_session_projects tiebreak test"
        );
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let unique = uuid::Uuid::new_v4();
    let sentinel_id = insert_project(&pool, SENTINEL_PROJECT_PATH).await?;
    let mut project_ids = [
        insert_project(&pool, &format!("tie-project-a-{unique}")).await?,
        insert_project(&pool, &format!("tie-project-b-{unique}")).await?,
    ];
    project_ids.sort_unstable();
    let [lower_id, higher_id] = project_ids;

    let session = insert_session(&pool, sentinel_id).await?;
    insert_request(&pool, session, lower_id).await?;
    insert_request(&pool, session, higher_id).await?;

    sqlx::raw_sql(MIGRATION_SQL).execute(&pool).await?;

    assert_eq!(
        session_project_id(&pool, session).await?,
        lower_id,
        "a tied request count must deterministically resolve to the lower project id"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
