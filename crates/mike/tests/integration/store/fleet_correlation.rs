use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::test_support::seed_project;
use sqlx::PgPool;
use uuid::Uuid;

use crate::test_db::TestDb;

async fn seed_session_at(
    pool: &PgPool,
    project_id: i64,
    session_uuid: Uuid,
    last_seen: DateTime<Utc>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar!(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, $3, $3)
         RETURNING id",
        session_uuid,
        project_id,
        last_seen,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

async fn seed_registration(
    pool: &PgPool,
    session_id: i64,
    project_id: i64,
    lease_expires_at: DateTime<Utc>,
    released_at: Option<DateTime<Utc>>,
) -> anyhow::Result<()> {
    sqlx::query!(
        "INSERT INTO workspace.session_registrations
            (session_id, project_id, cwd, lease_expires_at, released_at)
         VALUES ($1, $2, '/tmp/fleet-correlation-test', $3, $4)",
        session_id,
        project_id,
        lease_expires_at,
        released_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn fetch_fleet_nodes_includes_quiet_session_with_active_registration() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let project_id = seed_project(
        handle.pool(),
        &format!("fleet-correlation-active-{}", Uuid::new_v4()),
    )
    .await?;

    let quiet_session = Uuid::new_v4();
    let session_id = seed_session_at(
        handle.pool(),
        project_id,
        quiet_session,
        Utc::now() - Duration::hours(200),
    )
    .await?;
    seed_registration(
        handle.pool(),
        session_id,
        project_id,
        Utc::now() + Duration::minutes(10),
        None,
    )
    .await?;

    let nodes = handle.fetch_fleet_nodes(project_id, 24).await?;
    assert_eq!(
        nodes
            .iter()
            .filter(|n| n.session_uuid == quiet_session)
            .count(),
        1,
        "a session with an active (unreleased, unexpired) registration must survive a \
         24h window despite a 200h-old last_seen"
    );
    Ok(())
}

#[tokio::test]
async fn fetch_fleet_nodes_window_excludes_ended_and_expired_registrations() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let project_id = seed_project(
        handle.pool(),
        &format!("fleet-correlation-bounded-{}", Uuid::new_v4()),
    )
    .await?;

    let ended_session = Uuid::new_v4();
    let ended_id = seed_session_at(
        handle.pool(),
        project_id,
        ended_session,
        Utc::now() - Duration::hours(200),
    )
    .await?;
    seed_registration(
        handle.pool(),
        ended_id,
        project_id,
        Utc::now() + Duration::minutes(10),
        Some(Utc::now() - Duration::hours(1)),
    )
    .await?;

    let expired_session = Uuid::new_v4();
    let expired_id = seed_session_at(
        handle.pool(),
        project_id,
        expired_session,
        Utc::now() - Duration::hours(200),
    )
    .await?;
    seed_registration(
        handle.pool(),
        expired_id,
        project_id,
        Utc::now() - Duration::hours(1),
        None,
    )
    .await?;

    let recent_session = Uuid::new_v4();
    seed_session_at(handle.pool(), project_id, recent_session, Utc::now()).await?;

    let nodes = handle.fetch_fleet_nodes(project_id, 24).await?;
    let uuids: Vec<Uuid> = nodes.iter().map(|n| n.session_uuid).collect();
    assert!(
        !uuids.contains(&ended_session),
        "a released (ended) registration must not bypass the window"
    );
    assert!(
        !uuids.contains(&expired_session),
        "a lease-expired registration must not bypass the window"
    );
    assert!(
        uuids.contains(&recent_session),
        "a session active within the window must still appear"
    );
    Ok(())
}
