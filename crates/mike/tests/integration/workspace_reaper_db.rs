use crate::test_db::TestDb;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::workspace::RegisterWorkspaceSession;
use mike::workspace::Liveness;
use mike::workspace::boot_id::CURRENT_BOOT_ID;
use mike::workspace_reaper::sweep_once;

struct ReaperScenario {
    live_session: uuid::Uuid,
    dead_session: uuid::Uuid,
    null_pid_session: uuid::Uuid,
    stale_boot_session: uuid::Uuid,
}

async fn register_workspace_pid(
    handle: &PersistHandle,
    session_uuid: uuid::Uuid,
    project_path: &str,
    pid: Option<i32>,
) -> anyhow::Result<()> {
    handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid,
            project_path: Some(project_path),
            cwd: project_path,
            worktree: None,
            registered_via: "passive",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid,
        })
        .await?;
    Ok(())
}

async fn seed_reaper_scenario(
    handle: &PersistHandle,
    project_path: &str,
) -> anyhow::Result<ReaperScenario> {
    let live_session = uuid::Uuid::new_v4();
    let dead_session = uuid::Uuid::new_v4();
    let null_pid_session = uuid::Uuid::new_v4();
    let stale_boot_session = uuid::Uuid::new_v4();

    register_workspace_pid(
        handle,
        live_session,
        project_path,
        Some(std::process::id() as i32),
    )
    .await?;
    register_workspace_pid(handle, dead_session, project_path, Some(i32::MAX - 1)).await?;
    register_workspace_pid(handle, null_pid_session, project_path, None).await?;
    register_workspace_pid(handle, stale_boot_session, project_path, Some(i32::MAX - 2)).await?;

    #[rustfmt::skip]
    sqlx::query(
        "UPDATE workspace.session_registrations
            SET boot_id = 'a-previous-boot'
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(stale_boot_session)
    .execute(handle.pool())
    .await?;

    Ok(ReaperScenario {
        live_session,
        dead_session,
        null_pid_session,
        stale_boot_session,
    })
}

async fn assert_sweep_outcomes(
    handle: &PersistHandle,
    scenario: &ReaperScenario,
) -> anyhow::Result<()> {
    assert_eq!(
        handle.fetch_session_liveness(scenario.live_session).await?,
        Some(Liveness::Live),
        "A1: a live pid must survive the sweep"
    );
    assert_eq!(
        handle.fetch_session_liveness(scenario.dead_session).await?,
        Some(Liveness::Ended),
        "A2: a dead pid must be reaped"
    );
    assert_eq!(
        handle
            .fetch_session_liveness(scenario.null_pid_session)
            .await?,
        Some(Liveness::Live),
        "A3: a null pid must never be a reap candidate"
    );
    assert_eq!(
        handle
            .fetch_session_liveness(scenario.stale_boot_session)
            .await?,
        Some(Liveness::Live),
        "A4: a stale-boot pid must never be reaped even if it looks dead"
    );
    Ok(())
}

async fn assert_reap_provenance(
    handle: &PersistHandle,
    dead_session: uuid::Uuid,
    live_session: uuid::Uuid,
) -> anyhow::Result<()> {
    let (reaped_at, released_at): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT reaped_at, released_at
               FROM workspace.session_registrations
              WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(dead_session)
    .fetch_one(handle.pool())
    .await?;
    assert!(
        reaped_at.is_some() && released_at.is_some(),
        "A2: a reaped row stamps reaped_at alongside released_at"
    );

    let live_reaped_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT reaped_at
           FROM workspace.session_registrations
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(live_session)
    .fetch_one(handle.pool())
    .await?;
    assert!(
        live_reaped_at.is_none(),
        "a clean-live row must never carry reap provenance"
    );
    Ok(())
}

async fn set_lease_expiry(
    handle: &PersistHandle,
    session_uuid: uuid::Uuid,
    hours_from_now: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE workspace.session_registrations
            SET lease_expires_at = now() + make_interval(hours => $2)
          WHERE session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(session_uuid)
    .bind(hours_from_now)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn sweep_lapsed_lease_reaped_on_any_boot_and_recent_lapse_kept() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project_path = format!("/tmp/reaper-lapse-test-{}", uuid::Uuid::new_v4());

    let long_lapsed = uuid::Uuid::new_v4();
    let recently_lapsed = uuid::Uuid::new_v4();
    register_workspace_pid(&handle, long_lapsed, &project_path, None).await?;
    register_workspace_pid(&handle, recently_lapsed, &project_path, None).await?;
    set_lease_expiry(&handle, long_lapsed, -48).await?;
    set_lease_expiry(&handle, recently_lapsed, -1).await?;

    sweep_once(&handle, "a-boot-that-matches-nothing").await;

    assert_eq!(
        handle.fetch_session_liveness(long_lapsed).await?,
        Some(Liveness::Ended),
        "a lease lapsed for more than a day is reaped whatever its boot"
    );
    assert_ne!(
        handle.fetch_session_liveness(recently_lapsed).await?,
        Some(Liveness::Ended),
        "a lease lapsed within the grace stays unreleased"
    );

    register_workspace_pid(&handle, long_lapsed, &project_path, None).await?;
    assert_eq!(
        handle.fetch_session_liveness(long_lapsed).await?,
        Some(Liveness::Live),
        "a reaped lapsed row returns to live on re-register"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sweep_reaps_only_dead_current_boot_pids() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let Some(boot_id) = CURRENT_BOOT_ID.clone() else {
        db.cleanup().await?;
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;

    let project_path = format!("/tmp/reaper-test-{}", uuid::Uuid::new_v4());
    let scenario = seed_reaper_scenario(&handle, &project_path).await?;

    sweep_once(&handle, &boot_id).await;

    assert_sweep_outcomes(&handle, &scenario).await?;
    assert_reap_provenance(&handle, scenario.dead_session, scenario.live_session).await?;

    register_workspace_pid(
        &handle,
        scenario.dead_session,
        &project_path,
        Some(std::process::id() as i32),
    )
    .await?;
    assert_eq!(
        handle.fetch_session_liveness(scenario.dead_session).await?,
        Some(Liveness::Live),
        "A5: a reaped row returns to live on re-register"
    );

    db.cleanup().await?;
    Ok(())
}
