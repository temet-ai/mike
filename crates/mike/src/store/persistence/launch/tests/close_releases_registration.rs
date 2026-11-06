use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewLaunch;
use crate::store::persistence::test_support::{self, TestDbHandle};
use crate::store::persistence::workspace::RegisterWorkspaceSession;

const PROJECT: &str = "/tmp/close-releases-registration";

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture { db, handle }))
}

async fn register(handle: &PersistHandle, session: Uuid, role: &str) -> anyhow::Result<i64> {
    let registered = handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: Some(PROJECT),
            cwd: PROJECT,
            worktree: None,
            registered_via: "passive",
            intent: None,
            role: Some(role),
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;
    Ok(registered.project_id)
}

async fn released_at_is_set(handle: &PersistHandle, session: Uuid) -> anyhow::Result<bool> {
    let set = sqlx::query_scalar!(
        r#"SELECT r.released_at IS NOT NULL AS "released!"
             FROM workspace.session_registrations r
             JOIN sessions.sessions s ON s.id = r.session_id
            WHERE s.session_uuid = $1"#,
        session,
    )
    .fetch_one(handle.pool())
    .await?;
    Ok(set)
}

async fn launch_for(handle: &PersistHandle, session: Uuid) -> anyhow::Result<i64> {
    let mut new = NewLaunch::new("herdr", PROJECT);
    new.session_uuid = Some(session);
    Ok(handle.insert_launch(&new).await?)
}

#[tokio::test]
async fn record_launch_closed_releases_the_session_registration() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let sender = Uuid::new_v4();
    let seat = Uuid::new_v4();
    let project_id = register(&f.handle, sender, "coordinator").await?;
    register(&f.handle, seat, "worker").await?;
    let launch = launch_for(&f.handle, seat).await?;

    assert_eq!(
        f.handle
            .resolve_role_recipients(project_id, "worker", sender)
            .await?,
        vec![seat]
    );
    assert!(!released_at_is_set(&f.handle, seat).await?);

    f.handle
        .record_launch_closed(launch, "seat retired", Some(true))
        .await?;

    assert!(released_at_is_set(&f.handle, seat).await?);
    assert!(
        f.handle
            .resolve_role_recipients(project_id, "worker", sender)
            .await?
            .is_empty()
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn record_launch_closed_leaves_other_registrations_live() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let sender = Uuid::new_v4();
    let closed_seat = Uuid::new_v4();
    let other_seat = Uuid::new_v4();
    let project_id = register(&f.handle, sender, "coordinator").await?;
    register(&f.handle, closed_seat, "worker").await?;
    register(&f.handle, other_seat, "worker").await?;
    let launch = launch_for(&f.handle, closed_seat).await?;

    f.handle
        .record_launch_closed(launch, "seat retired", None)
        .await?;

    assert_eq!(
        f.handle
            .resolve_role_recipients(project_id, "worker", sender)
            .await?,
        vec![other_seat]
    );
    f.db.cleanup().await
}

#[tokio::test]
async fn record_launch_closed_without_a_registration_still_closes() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let launch = launch_for(&f.handle, Uuid::new_v4()).await?;

    f.handle
        .record_launch_closed(launch, "seat retired", Some(true))
        .await?;

    let missing = f
        .handle
        .record_launch_closed(launch + 1_000_000, "seat retired", None)
        .await;
    assert!(missing.is_err());
    f.db.cleanup().await
}
