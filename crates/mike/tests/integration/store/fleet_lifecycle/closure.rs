use chrono::{DateTime, Utc};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch::NewLaunch;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::{fixture, opened_launch, running, stopping};

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct Closure {
    closed_at: Option<DateTime<Utc>>,
    close_reason: Option<String>,
    close_process_exited: Option<bool>,
    released_at: Option<DateTime<Utc>>,
}

async fn registered_launch(handle: &PersistHandle, run: &str) -> anyhow::Result<(i64, Uuid)> {
    let id = opened_launch(handle, run).await?;
    let session: Uuid =
        sqlx::query_scalar("SELECT session_uuid FROM launch.launches WHERE id = $1")
            .bind(id)
            .fetch_one(handle.pool())
            .await?;
    handle
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    sqlx::query(
        "INSERT INTO workspace.session_registrations
             (session_id, project_id, cwd, registered_via, lease_expires_at)
         SELECT id, project_id, '/tmp/fleet-closure', 'mcp', now() + interval '15 minutes'
           FROM sessions.sessions WHERE session_uuid = $1",
    )
    .bind(session)
    .execute(handle.pool())
    .await?;
    Ok((id, session))
}

async fn closure(handle: &PersistHandle, id: i64) -> anyhow::Result<Closure> {
    Ok(sqlx::query_as(
        "SELECT l.closed_at, l.close_reason, l.close_process_exited, r.released_at
           FROM launch.launches l
           JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
           JOIN workspace.session_registrations r ON r.session_id = s.id
          WHERE l.id = $1",
    )
    .bind(id)
    .fetch_one(handle.pool())
    .await?)
}

async fn unverified_closure(process_exited: Option<bool>) -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("fleet-closure-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let (id, session) = registered_launch(&f.handle, &run).await?;
    assert!(
        f.handle
            .launch()
            .insert_message(Uuid::new_v4(), session, None, "before pane closure")
            .await?
            .is_some()
    );
    let stopped = stopping(&f.handle, &row).await?;
    f.handle
        .fleet()
        .record_fleet_launch_closure(id, &run, "pane closed", process_exited)
        .await?;
    let closed = assert_unverified(&f.handle, id, session, &run, process_exited, &stopped).await?;
    let verified = assert_verified(&f.handle, id, &run, &stopped, &closed).await?;
    assert_late_replay(&f.handle, id, session, &run, &verified).await?;
    f.db.cleanup().await
}

async fn assert_unverified(
    handle: &PersistHandle,
    id: i64,
    session: Uuid,
    run: &str,
    process_exited: Option<bool>,
    stopped: &mike::store::persistence::fleet::FleetRow,
) -> anyhow::Result<Closure> {
    let closed = closure(handle, id).await?;
    assert!(closed.closed_at.is_some());
    assert_eq!(closed.close_reason.as_deref(), Some("pane closed"));
    assert_eq!(closed.close_process_exited, process_exited);
    assert!(closed.released_at.is_none());
    assert!(
        handle
            .launch()
            .insert_message(Uuid::new_v4(), session, None, "must refuse closed pane")
            .await?
            .is_none()
    );
    assert_eq!(handle.fleet().unsettled_fleet_launches(run).await?.len(), 1);
    assert!(
        handle
            .fleet()
            .complete_fleet_run(stopped, None)
            .await?
            .is_none()
    );
    Ok(closed)
}

async fn assert_verified(
    handle: &PersistHandle,
    id: i64,
    run: &str,
    stopped: &mike::store::persistence::fleet::FleetRow,
    closed: &Closure,
) -> anyhow::Result<Closure> {
    handle
        .fleet()
        .record_fleet_launch_verified_exit(id, run, "later observed exit")
        .await?;
    let verified = closure(handle, id).await?;
    assert_eq!(verified.closed_at, closed.closed_at);
    assert_eq!(verified.close_reason, closed.close_reason);
    assert_eq!(verified.close_process_exited, Some(true));
    assert!(verified.released_at.is_some());
    assert!(
        handle
            .fleet()
            .unsettled_fleet_launches(run)
            .await?
            .is_empty()
    );
    assert!(
        handle
            .fleet()
            .complete_fleet_run(stopped, None)
            .await?
            .is_some()
    );
    Ok(verified)
}

async fn assert_late_replay(
    handle: &PersistHandle,
    id: i64,
    session: Uuid,
    run: &str,
    verified: &Closure,
) -> anyhow::Result<()> {
    for late in [Some(false), None, Some(true)] {
        handle
            .fleet()
            .record_fleet_launch_closure(id, run, "late replay", late)
            .await?;
        assert_eq!(&closure(handle, id).await?, verified);
    }
    assert!(
        handle
            .launch()
            .insert_message(Uuid::new_v4(), session, None, "still closed")
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn false_exit_closes_mail_but_retains_registration_and_blocks_completion()
-> anyhow::Result<()> {
    unverified_closure(Some(false)).await
}

#[tokio::test]
async fn unknown_exit_closes_mail_but_retains_registration_and_blocks_completion()
-> anyhow::Result<()> {
    unverified_closure(None).await
}

#[tokio::test]
async fn closure_evidence_moves_unknown_to_false_and_never_back() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("fleet-evidence-{}", Uuid::new_v4());
    running(&f.handle, &run).await?;
    let (id, _) = registered_launch(&f.handle, &run).await?;
    f.handle
        .fleet()
        .record_fleet_launch_closure(id, &run, "pane closed", None)
        .await?;
    let unknown = closure(&f.handle, id).await?;
    f.handle
        .fleet()
        .record_fleet_launch_closure(id, &run, "still alive", Some(false))
        .await?;
    let live = closure(&f.handle, id).await?;
    assert_eq!(live.closed_at, unknown.closed_at);
    assert_eq!(live.close_reason, unknown.close_reason);
    assert_eq!(live.close_process_exited, Some(false));
    assert!(live.released_at.is_none());
    f.handle
        .fleet()
        .record_fleet_launch_closure(id, &run, "unknown replay", None)
        .await?;
    assert_eq!(closure(&f.handle, id).await?, live);
    f.db.cleanup().await
}

#[tokio::test]
async fn closure_refuses_wrong_run_missing_id_and_unopened_reservations() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("fleet-closure-owner-{}", Uuid::new_v4());
    let row = running(&f.handle, &run).await?;
    let (id, _) = registered_launch(&f.handle, &run).await?;
    let before = closure(&f.handle, id).await?;
    assert!(
        f.handle
            .fleet()
            .record_fleet_launch_closure(id, "fleet-wrong", "wrong owner", Some(true))
            .await
            .is_err()
    );
    assert_eq!(closure(&f.handle, id).await?, before);
    assert!(
        f.handle
            .fleet()
            .record_fleet_launch_closure(-1, &run, "missing", Some(true))
            .await
            .is_err()
    );
    let mut pending = NewLaunch::new("herdr", "/tmp/pending-closure");
    pending.run_id = Some(run.clone());
    let pending_id = f.handle.launch().insert_launch(&pending).await?;
    assert!(
        f.handle
            .fleet()
            .record_fleet_launch_closure(pending_id, &run, "not opened", None)
            .await
            .is_err()
    );
    assert!(
        f.handle
            .launch()
            .record_launch_closed(pending_id, "queued replay", Some(true))
            .await
            .is_err()
    );
    let pending_state: (bool, String, bool) = sqlx::query_as(
        "SELECT opened, result, closed_at IS NOT NULL FROM launch.launches WHERE id = $1",
    )
    .bind(pending_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(pending_state, (false, "pending".into(), false));
    let stopped = stopping(&f.handle, &row).await?;
    assert!(
        f.handle
            .fleet()
            .complete_fleet_run(&stopped, None)
            .await?
            .is_none()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn concurrent_closure_evidence_cannot_downgrade_verified_exit() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = format!("fleet-concurrent-closure-{}", Uuid::new_v4());
    running(&f.handle, &run).await?;
    let (id, _) = registered_launch(&f.handle, &run).await?;
    let (unknown, live, exited) = tokio::join!(
        f.handle
            .fleet()
            .record_fleet_launch_closure(id, &run, "unknown", None),
        f.handle
            .fleet()
            .record_fleet_launch_closure(id, &run, "alive", Some(false)),
        f.handle
            .fleet()
            .record_fleet_launch_verified_exit(id, &run, "exited"),
    );
    unknown?;
    live?;
    exited?;
    let result = closure(&f.handle, id).await?;
    assert_eq!(result.close_process_exited, Some(true));
    assert!(result.released_at.is_some());
    assert!(
        f.handle
            .fleet()
            .unsettled_fleet_launches(&run)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn central_closure_routing_preserves_fleet_proof_and_unrelated_behavior() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let fleet_run = format!("fleet-routed-{}", Uuid::new_v4());
    let other_run = format!("other-routed-{}", Uuid::new_v4());
    running(&f.handle, &fleet_run).await?;
    f.handle
        .launch()
        .upsert_run(&mike::store::persistence::launch::NewRun {
            run_id: other_run.clone(),
            project_path: None,
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let (fleet_id, _) = registered_launch(&f.handle, &fleet_run).await?;
    let (other_id, _) = registered_launch(&f.handle, &other_run).await?;
    f.handle
        .launch()
        .record_launch_closed(fleet_id, "fleet pane closed", Some(false))
        .await?;
    let unverified = closure(&f.handle, fleet_id).await?;
    assert_eq!(unverified.close_process_exited, Some(false));
    assert!(unverified.released_at.is_none());
    f.handle
        .launch()
        .record_launch_closed(other_id, "ordinary closure", None)
        .await?;
    assert!(closure(&f.handle, other_id).await?.released_at.is_some());
    assert_eq!(closure(&f.handle, fleet_id).await?, unverified);
    f.handle
        .launch()
        .record_launch_closed(fleet_id, "later verified exit", Some(true))
        .await?;
    let verified = closure(&f.handle, fleet_id).await?;
    assert_eq!(verified.close_process_exited, Some(true));
    assert!(verified.released_at.is_some());
    for late in [None, Some(false)] {
        f.handle
            .launch()
            .record_launch_closed(fleet_id, "late queued replay", late)
            .await?;
        assert_eq!(closure(&f.handle, fleet_id).await?, verified);
    }
    assert!(
        f.handle
            .launch()
            .record_launch_closed(-1, "missing", None)
            .await
            .is_err()
    );
    f.db.cleanup().await
}
