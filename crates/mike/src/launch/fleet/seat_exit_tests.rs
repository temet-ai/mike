use super::observe;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::UnsettledFleetLaunch;
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
