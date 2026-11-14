use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support as test_util;
use crate::store::persistence::workspace::RegisterWorkspaceSession;

fn registration<'a>(
    session_uuid: uuid::Uuid,
    cwd: &'a str,
    via: &'a str,
) -> RegisterWorkspaceSession<'a> {
    RegisterWorkspaceSession {
        session_uuid,
        project_path: Some(cwd),
        cwd,
        worktree: None,
        registered_via: via,
        intent: None,
        role: None,
        name: None,
        instructions: None,
        pid: None,
    }
}

async fn recorded(
    handle: &PersistHandle,
    session_uuid: uuid::Uuid,
) -> anyhow::Result<(String, String, String)> {
    Ok(sqlx::query_as(
        "SELECT s.cwd, r.cwd, r.registered_via
           FROM sessions.sessions s
           JOIN workspace.session_registrations r ON r.session_id = s.id
          WHERE s.session_uuid = $1",
    )
    .bind(session_uuid)
    .fetch_one(&handle.pool)
    .await?)
}

#[tokio::test]
async fn passive_registration_after_the_launcher_keeps_the_launchers_folder() -> anyhow::Result<()>
{
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4();
    let exact = format!("/tmp/passive-keeps-{session}/forge/buildgate");
    let guessed = format!("/tmp/passive-keeps-{session}");
    handle
        .register_workspace_session(registration(session, &exact, "launcher"))
        .await?;
    handle
        .register_workspace_session(registration(session, &guessed, "passive"))
        .await?;
    assert_eq!(
        recorded(&handle, session).await?,
        (exact.clone(), exact, "launcher".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn the_launcher_replaces_an_earlier_passive_guess() -> anyhow::Result<()> {
    let Some(db) = test_util::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = uuid::Uuid::new_v4();
    let exact = format!("/tmp/passive-first-{session}/forge/hyprlint");
    let guessed = format!("/tmp/passive-first-{session}");
    handle
        .register_workspace_session(registration(session, &guessed, "passive"))
        .await?;
    handle
        .register_workspace_session(registration(session, &exact, "launcher"))
        .await?;
    assert_eq!(
        recorded(&handle, session).await?,
        (exact.clone(), exact, "launcher".to_owned())
    );
    Ok(())
}
