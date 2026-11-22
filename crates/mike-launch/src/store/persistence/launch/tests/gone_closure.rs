use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::{LaunchSurfaceRefs, NewLaunch};
use crate::store::persistence::test_support::{self, TestDbHandle, seed};
use crate::store::persistence::workspace::RegisterWorkspaceSession;

const PROJECT: &str = "/tmp/gone-closure";

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
}

type Closure = (bool, Option<String>, Option<bool>);

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}

fn day_ago() -> DateTime<Utc> {
    Utc::now() - Duration::hours(24)
}

fn after_now() -> DateTime<Utc> {
    Utc::now() + Duration::minutes(1)
}

async fn register(handle: &PersistHandle, session: Uuid) -> anyhow::Result<()> {
    handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: Some(PROJECT),
            cwd: PROJECT,
            worktree: None,
            registered_via: "launcher",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    Ok(())
}

async fn launch(
    handle: &PersistHandle,
    session: Option<Uuid>,
    opened: bool,
) -> anyhow::Result<i64> {
    let mut new = NewLaunch::new("herdr", PROJECT);
    new.session_uuid = session;
    let id = handle.launch().insert_launch(&new).await?;
    if opened {
        let surface = LaunchSurfaceRefs {
            surface_ref: Some(format!("pane-{id}")),
            pid: Some(i32::MAX - 1),
            ..LaunchSurfaceRefs::default()
        };
        handle.launch().record_launch_opened(id, &surface).await?;
    }
    Ok(id)
}

async fn closure(handle: &PersistHandle, id: i64) -> anyhow::Result<Closure> {
    Ok(sqlx::query_as(
        "SELECT closed_at IS NOT NULL, close_reason, close_process_exited
           FROM launch.launches WHERE id = $1",
    )
    .bind(id)
    .fetch_one(handle.pool())
    .await?)
}

async fn session_row_id(handle: &PersistHandle, session: Uuid) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?,
    )
}

async fn backdate_launch(handle: &PersistHandle, id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE launch.launches SET created_at = now() - interval '2 days' WHERE id = $1")
        .bind(id)
        .execute(handle.pool())
        .await?;
    Ok(())
}

fn open() -> Closure {
    (false, None, None)
}

fn closed(reason: &str, exited: Option<bool>) -> Closure {
    (true, Some(reason.to_owned()), exited)
}

#[tokio::test]
async fn close_gone_launches_session_ended_closes_as_session_ended() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    register(&f.handle, session).await?;
    let id = launch(&f.handle, Some(session), true).await?;
    f.handle.end_workspace_registration(session, None).await?;

    assert_eq!(f.handle.launch().close_gone_launches(day_ago()).await?, 1);

    assert_eq!(
        closure(&f.handle, id).await?,
        closed("session ended", Some(true))
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn close_gone_launches_session_reaped_closes_as_process_reaped() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    register(&f.handle, session).await?;
    let id = launch(&f.handle, Some(session), true).await?;
    let row = session_row_id(&f.handle, session).await?;
    f.handle.reap_dead_registrations(&[row]).await?;

    f.handle.launch().close_gone_launches(day_ago()).await?;

    assert_eq!(
        closure(&f.handle, id).await?,
        closed("process reaped", Some(true))
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn close_gone_launches_live_session_stays_open() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    register(&f.handle, session).await?;
    let id = launch(&f.handle, Some(session), true).await?;

    assert_eq!(f.handle.launch().close_gone_launches(after_now()).await?, 0);

    assert_eq!(closure(&f.handle, id).await?, open());
    f.db.cleanup().await
}

#[tokio::test]
async fn close_gone_launches_unopened_launch_closes_as_never_started_once_quiet()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = launch(&f.handle, Some(Uuid::new_v4()), false).await?;

    f.handle.launch().close_gone_launches(day_ago()).await?;
    assert_eq!(closure(&f.handle, id).await?, open());

    f.handle.launch().close_gone_launches(after_now()).await?;
    assert_eq!(
        closure(&f.handle, id).await?,
        closed("never started", Some(true))
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn close_gone_launches_opened_launch_without_session_closes_with_exit_unknown()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let with_uuid = launch(&f.handle, Some(Uuid::new_v4()), true).await?;
    let without_uuid = launch(&f.handle, None, true).await?;

    f.handle.launch().close_gone_launches(day_ago()).await?;
    assert_eq!(closure(&f.handle, with_uuid).await?, open());

    f.handle.launch().close_gone_launches(after_now()).await?;
    for id in [with_uuid, without_uuid] {
        assert_eq!(
            closure(&f.handle, id).await?,
            closed("session never reached mike", None)
        );
    }
    f.db.cleanup().await
}

#[tokio::test]
async fn close_gone_launches_resume_of_an_ended_session_stays_open_until_quiet()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    register(&f.handle, session).await?;
    f.handle.end_workspace_registration(session, None).await?;
    let id = launch(&f.handle, Some(session), true).await?;

    f.handle.launch().close_gone_launches(day_ago()).await?;
    assert_eq!(closure(&f.handle, id).await?, open());

    f.handle.launch().close_gone_launches(after_now()).await?;
    assert_eq!(
        closure(&f.handle, id).await?,
        closed("session ended", Some(true))
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn close_gone_launches_recent_request_keeps_unregistered_launch_open() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let project = seed::seed_project(f.handle.pool(), PROJECT).await?;
    seed::seed_captured_request(
        f.handle.pool(),
        &session.to_string(),
        project,
        "primary",
        b"{}",
    )
    .await?;
    let id = launch(&f.handle, Some(session), true).await?;
    backdate_launch(&f.handle, id).await?;

    f.handle.launch().close_gone_launches(day_ago()).await?;
    assert_eq!(closure(&f.handle, id).await?, open());

    f.handle.launch().close_gone_launches(after_now()).await?;
    assert_eq!(
        closure(&f.handle, id).await?,
        closed("session went quiet", None)
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn close_gone_launches_already_closed_launch_keeps_its_reason() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = launch(&f.handle, None, false).await?;
    f.handle
        .launch()
        .record_launch_closed(id, "seat retired", Some(true))
        .await?;

    assert_eq!(f.handle.launch().close_gone_launches(after_now()).await?, 0);

    assert_eq!(
        closure(&f.handle, id).await?,
        closed("seat retired", Some(true))
    );
    f.db.cleanup().await
}
