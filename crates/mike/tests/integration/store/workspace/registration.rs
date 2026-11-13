use mike::extract::request::{compute_project_root, project_display_name};
use mike::store::persistence::PersistError;
use mike::store::persistence::SENTINEL_PROJECT_PATH;
use mike::store::persistence::id_resolvers::resolve_session_id;
use mike::store::persistence::proxy_repo::ProxyPersist as _;
use mike::store::persistence::workspace::{EndRegistrationOutcome, RegisterWorkspaceSession};
use mike::workspace::{Liveness, RosterScope};

use crate::store::workspace::support::{register_input, shared_handle, unique};

#[tokio::test]
async fn register_end_roster_lifecycle() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_path = unique("workspace-test");
    let session_a = uuid::Uuid::new_v4();
    let session_b = uuid::Uuid::new_v4();

    let registered_a = handle
        .register_workspace_session(RegisterWorkspaceSession {
            intent: Some(("fix the flaky test", "rerun with --retries")),
            ..register_input(session_a, &project_path)
        })
        .await?;
    assert_eq!(registered_a.registered_via, "hook");
    assert!(registered_a.lease_expires_at > chrono::Utc::now());

    let registered_b = handle
        .register_workspace_session(RegisterWorkspaceSession {
            cwd: &format!("{project_path}/.claude/worktrees/xyz"),
            worktree: Some("xyz"),
            ..register_input(session_b, &project_path)
        })
        .await?;
    assert_eq!(
        registered_a.project_id, registered_b.project_id,
        "both sessions in the same repo resolve to the same project row"
    );

    let roster = handle
        .proxy()
        .fetch_workspace_roster(registered_a.project_id, RosterScope::All)
        .await?;
    let row_a = roster
        .iter()
        .find(|r| r.session_uuid == session_a)
        .ok_or_else(|| anyhow::anyhow!("session A missing from roster"))?;
    assert!(matches!(row_a.liveness, Liveness::Live));
    assert_eq!(row_a.goal.as_deref(), Some("fix the flaky test"));
    let row_b = roster
        .iter()
        .find(|r| r.session_uuid == session_b)
        .ok_or_else(|| anyhow::anyhow!("session B missing from roster"))?;
    assert!(matches!(row_b.liveness, Liveness::Live));
    assert_eq!(row_b.worktree.as_deref(), Some("xyz"));
    assert!(row_b.goal.is_none(), "session B supplied no intent");

    handle.end_workspace_registration(session_a, None).await?;
    let roster = handle
        .proxy()
        .fetch_workspace_roster(registered_a.project_id, RosterScope::All)
        .await?;
    let row_a = roster
        .iter()
        .find(|r| r.session_uuid == session_a)
        .ok_or_else(|| anyhow::anyhow!("session A missing from roster after end"))?;
    assert!(matches!(row_a.liveness, Liveness::Ended));
    assert!(row_a.released_at.is_some());
    let row_b = roster
        .iter()
        .find(|r| r.session_uuid == session_b)
        .ok_or_else(|| anyhow::anyhow!("session B missing from roster"))?;
    assert!(matches!(row_b.liveness, Liveness::Live));

    let second_end = handle.end_workspace_registration(session_a, None).await;
    assert!(matches!(
        second_end,
        Err(PersistError::RegistrationNotFound(uuid)) if uuid == session_a
    ));

    let found = handle.find_project_id_by_path(&project_path).await?;
    assert_eq!(found, Some(registered_a.project_id));
    let missing = handle.find_project_id_by_path("/no/such/project").await?;
    assert_eq!(missing, None);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn register_rejects_invalid_registered_via() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let project_path = unique("workspace-test");
    let session = uuid::Uuid::new_v4();

    let result = handle
        .register_workspace_session(RegisterWorkspaceSession {
            registered_via: "carrier-pigeon",
            ..register_input(session, &project_path)
        })
        .await;
    assert!(matches!(
        result,
        Err(PersistError::InvalidRegisteredVia(v)) if v == "carrier-pigeon"
    ));

    let found = handle.find_project_id_by_path(&project_path).await?;
    assert_eq!(
        found, None,
        "a rejected registered_via must not create a project row either"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn end_and_renew_lifecycle_covers_unknown_pid_mismatch_and_post_end_404s()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let unknown = uuid::Uuid::new_v4();
    let result = handle.end_workspace_registration(unknown, None).await;
    assert!(matches!(
        result,
        Err(PersistError::RegistrationNotFound(uuid)) if uuid == unknown
    ));

    let project_path = unique("workspace-test");
    let session = uuid::Uuid::new_v4();
    let registered_pid = 12345;

    let registered = handle
        .register_workspace_session(RegisterWorkspaceSession {
            pid: Some(registered_pid),
            ..register_input(session, &project_path)
        })
        .await?;

    let renewed = handle.renew_workspace_lease(session).await?;
    assert!(
        renewed.lease_expires_at >= registered.lease_expires_at,
        "heartbeat must not shorten the lease"
    );

    let rejected = handle
        .end_workspace_registration(session, Some(99999))
        .await?;
    assert_eq!(rejected, EndRegistrationOutcome::Rejected);
    assert_eq!(
        handle.proxy().fetch_session_liveness(session).await?,
        Some(Liveness::Live),
        "a pid-mismatched end must leave the registration live"
    );

    let ended = handle
        .end_workspace_registration(session, Some(registered_pid))
        .await?;
    assert_eq!(ended, EndRegistrationOutcome::Ended);
    assert_eq!(
        handle.proxy().fetch_session_liveness(session).await?,
        Some(Liveness::Ended)
    );

    let after_end = handle.renew_workspace_lease(session).await;
    assert!(matches!(
        after_end,
        Err(PersistError::RegistrationNotFound(uuid)) if uuid == session
    ));

    let never_registered = uuid::Uuid::new_v4();
    let unknown_renew = handle.renew_workspace_lease(never_registered).await;
    assert!(matches!(
        unknown_renew,
        Err(PersistError::RegistrationNotFound(uuid)) if uuid == never_registered
    ));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn register_by_absolute_cwd_resolves_same_project_as_capture() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let unique = uuid::Uuid::new_v4();
    let absolute_cwd = format!("/Users/tester-{unique}/projects/temet/temet-proxy");
    let canonical = project_display_name(compute_project_root(&absolute_cwd));

    let capture_project_id = handle.upsert_project_returning_id(&canonical).await?;

    let session = uuid::Uuid::new_v4();
    let registered = handle
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid: session,
            project_path: Some(&canonical),
            cwd: &absolute_cwd,
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;

    assert_eq!(
        registered.project_id, capture_project_id,
        "registration(absolute cwd) and capture(trimmed) must resolve to one project row"
    );

    let row_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM projects.projects WHERE project_path = $1")
            .bind(&canonical)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(row_count, 1, "no duplicate project row for the same repo");

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn register_heals_session_off_sentinel_when_quota_probe_preceded_it() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let session = uuid::Uuid::new_v4();
    resolve_session_id(handle.pool(), &session.to_string(), None, None).await?;

    let project_path = unique("sentinel-heal-test");
    let registered = handle
        .register_workspace_session(register_input(session, &project_path))
        .await?;

    let session_project_id: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        session_project_id, registered.project_id,
        "the session row must be healed off the sentinel onto the registered project"
    );

    let sentinel_project_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM projects.projects WHERE project_path = $1")
            .bind(SENTINEL_PROJECT_PATH)
            .fetch_optional(handle.pool())
            .await?;
    assert_ne!(
        Some(session_project_id),
        sentinel_project_id,
        "the session must no longer point at the sentinel project"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn register_does_not_repoint_a_session_already_on_a_real_project() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };

    let session = uuid::Uuid::new_v4();
    let original_project_path = unique("sentinel-heal-original");

    resolve_session_id(handle.pool(), &session.to_string(), None, None).await?;
    let assignment = handle
        .assign_session_project(session, &original_project_path)
        .await
        .map_err(|e| anyhow::anyhow!("assign_session_project failed: {e:?}"))?;

    let other_project_path = unique("sentinel-heal-other");
    handle
        .register_workspace_session(register_input(session, &other_project_path))
        .await?;

    let session_project_id: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(
        session_project_id, assignment.project_id,
        "a session already assigned by an operator must never be re-pointed by a later (automatic) registration"
    );

    handle.pool().close().await;
    Ok(())
}
