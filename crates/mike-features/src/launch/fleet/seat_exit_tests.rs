use super::observe;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::UnsettledFleetLaunch;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support as test_util;
use crate::store::persistence::workspace::RegisterWorkspaceSession;
use uuid::Uuid;

fn seat(session: Option<Uuid>) -> UnsettledFleetLaunch {
    UnsettledFleetLaunch {
        id: 1,
        session_uuid: session,
        backend: "herdr-core".into(),
        seat: Some("worker".into()),
        surface_ref: None,
        pid: None,
        opened: true,
        result: "opened".into(),
        opened_at: None,
        closed_at: None,
        close_process_exited: None,
    }
}

async fn register(handle: &PersistHandle, session: Uuid) -> anyhow::Result<()> {
    let path = "/tmp/seat-exit";
    handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: Some(path),
            cwd: path,
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_seat_whose_session_ended_is_exited_and_a_live_one_is_not() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let ended = Uuid::new_v4();
    let live = Uuid::new_v4();
    register(&handle, ended).await?;
    register(&handle, live).await?;
    handle.end_workspace_registration(ended, None).await?;
    let run = "fleet-seat-exit";
    assert_eq!(observe(&handle, run, &seat(Some(ended))).await?, Some(true));
    assert_eq!(observe(&handle, run, &seat(Some(live))).await?, None);
    let mut never_opened = seat(Some(ended));
    never_opened.opened = false;
    assert_eq!(observe(&handle, run, &never_opened).await?, None);
    assert_eq!(observe(&handle, run, &seat(None)).await?, None);
    assert_eq!(
        observe(&handle, run, &seat(Some(Uuid::new_v4()))).await?,
        None
    );
    db.cleanup().await?;
    Ok(())
}

async fn unopened_launch(handle: &PersistHandle, session: Uuid, run: &str) -> anyhow::Result<i64> {
    let mut new = crate::store::persistence::launch::NewLaunch::new("herdr", "/tmp/seat-exit");
    new.session_uuid = Some(session);
    new.run_id = Some(run.into());
    new.seat = Some("worker".into());
    let id = handle.launch().insert_launch(&new).await?;
    sqlx::query("UPDATE launch.launches SET created_at = now() - interval '1 hour' WHERE id = $1")
        .bind(id)
        .execute(handle.pool())
        .await?;
    Ok(id)
}

async fn make_requests(handle: &PersistHandle, session: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO requests.requests (
             session_id, project_id, method_id, path, is_streaming, message_count,
             is_tool_loop, status_id, params, last_user_message, human_prompt,
             turn_type_id, turn_origin_id, created_at, completed_at)
         SELECT s.id, s.project_id, rm.id, '/v1/messages', true, 1, false, rs.id, '{}'::jsonb,
                '', '', rtt.id, rto.id, NOW(), NOW()
           FROM sessions.sessions s, requests.methods rm, requests.statuses rs,
                requests.turn_types rtt, requests.turn_origins rto
          WHERE s.session_uuid = $1 AND rm.code = 'POST' AND rs.code = 'complete'
            AND rtt.code = 'user_prompt' AND rto.code = 'primary'",
    )
    .bind(session)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_never_opened_launch_closes_when_its_session_ended_even_with_requests()
-> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let run = "fleet-unopened";
    sqlx::query("INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'herdr')")
        .bind(run)
        .execute(handle.pool())
        .await?;
    let ended = Uuid::new_v4();
    let live = Uuid::new_v4();
    register(&handle, ended).await?;
    register(&handle, live).await?;
    make_requests(&handle, ended).await?;
    make_requests(&handle, live).await?;
    handle.end_workspace_registration(ended, None).await?;
    let ended_launch = unopened_launch(&handle, ended, run).await?;
    let live_launch = unopened_launch(&handle, live, run).await?;
    assert!(
        handle
            .fleet()
            .close_never_started_fleet_launch(ended_launch, run, "never started", 300.0)
            .await?
    );
    assert!(
        !handle
            .fleet()
            .close_never_started_fleet_launch(live_launch, run, "never started", 300.0)
            .await?
    );
    assert_eq!(handle.fleet().unsettled_fleet_launches(run).await?.len(), 1);
    db.cleanup().await?;
    Ok(())
}
