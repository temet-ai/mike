use mike::store::persistence::PersistError;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use mike::store::persistence::mailbox::SessionIdResolution;
use mike::store::persistence::session_goals::SetSessionIdentity;
use mike::store::persistence::workspace::RegisterWorkspaceSession;
use uuid::Uuid;

use super::workspace::support::{register_input, shared_handle, unique};

fn identity<'a>(session_uuid: Uuid) -> SetSessionIdentity<'a> {
    SetSessionIdentity {
        session_uuid,
        display_name: None,
        role: None,
        goal: None,
        next_steps: None,
        instructions: None,
        set_by: "mcp",
    }
}

async fn registered(handle: &PersistHandle, project_path: &str) -> anyhow::Result<(Uuid, i64)> {
    let session = Uuid::new_v4();
    let row = handle
        .register_workspace_session(register_input(session, project_path))
        .await?;
    Ok((session, row.project_id))
}

async fn seed_launch_identity(
    handle: &PersistHandle,
    session: Uuid,
    identity: serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO launch.launches (session_uuid, backend, folder, identity)
         VALUES ($1, 'none', '/tmp/spawn', $2)",
    )
    .bind(session)
    .bind(identity)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn set_session_goal_partial_update_keeps_every_other_identity_field() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let (session, _) = registered(&handle, &unique("identity-partial")).await?;

    handle
        .launch()
        .set_session_goal(SetSessionIdentity {
            role: Some("rust-developer"),
            goal: Some("ship the schema"),
            next_steps: Some("write the tests"),
            instructions: Some("implement sections 1 and 2 only"),
            ..identity(session)
        })
        .await?;
    handle
        .launch()
        .set_session_goal(SetSessionIdentity {
            display_name: Some("rust-dev-alpha"),
            ..identity(session)
        })
        .await?;

    let goal = handle
        .launch()
        .get_session_goal(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session_goal missing after set"))?;
    assert_eq!(goal.display_name.as_deref(), Some("rust-dev-alpha"));
    assert_eq!(goal.role.as_deref(), Some("rust-developer"));
    assert_eq!(goal.goal, "ship the schema");
    assert_eq!(goal.next_steps, "write the tests");
    assert_eq!(
        goal.instructions.as_deref(),
        Some("implement sections 1 and 2 only")
    );
    Ok(())
}

#[tokio::test]
async fn set_session_goal_for_an_unknown_session_is_refused() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let outcome = handle
        .launch()
        .set_session_goal(SetSessionIdentity {
            display_name: Some("ghost"),
            ..identity(Uuid::new_v4())
        })
        .await;
    assert!(matches!(outcome, Err(PersistError::DataInvariant(_))));
    Ok(())
}

#[tokio::test]
async fn launch_seed_applies_at_registration_and_never_overrides_a_declared_role()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let project_path = unique("identity-seed");
    let session = Uuid::new_v4();
    seed_launch_identity(
        &handle,
        session,
        serde_json::json!({"role": "observer", "name": "observer-1", "instructions": "watch"}),
    )
    .await?;

    handle
        .register_workspace_session(register_input(session, &project_path))
        .await?;
    let seeded = handle
        .launch()
        .get_session_goal(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded identity missing"))?;
    assert_eq!(seeded.role.as_deref(), Some("observer"));
    assert_eq!(seeded.display_name.as_deref(), Some("observer-1"));
    assert_eq!(seeded.instructions.as_deref(), Some("watch"));

    handle
        .launch()
        .set_session_goal(SetSessionIdentity {
            role: Some("reviewer"),
            ..identity(session)
        })
        .await?;
    handle
        .register_workspace_session(register_input(session, &project_path))
        .await?;
    let after = handle
        .launch()
        .get_session_goal(session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("identity missing after re-registration"))?;
    assert_eq!(after.role.as_deref(), Some("reviewer"));

    let explicit = Uuid::new_v4();
    seed_launch_identity(&handle, explicit, serde_json::json!({"role": "observer"})).await?;
    handle
        .register_workspace_session(RegisterWorkspaceSession {
            role: Some("coordinator"),
            ..register_input(explicit, &project_path)
        })
        .await?;
    let explicit_goal = handle
        .launch()
        .get_session_goal(explicit)
        .await?
        .ok_or_else(|| anyhow::anyhow!("explicit identity missing"))?;
    assert_eq!(explicit_goal.role.as_deref(), Some("coordinator"));
    Ok(())
}

#[tokio::test]
async fn resolve_name_recipient_is_case_insensitive_and_reports_ambiguity() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let project_path = unique("identity-name");
    let (sender, project_id) = registered(&handle, &project_path).await?;
    let (named, _) = registered(&handle, &project_path).await?;
    let (dup_a, _) = registered(&handle, &project_path).await?;
    let (dup_b, _) = registered(&handle, &project_path).await?;
    let names = [(named, "Observer-1"), (dup_a, "worker"), (dup_b, "worker")];
    for (session, name) in names {
        handle
            .launch()
            .set_session_goal(SetSessionIdentity {
                display_name: Some(name),
                ..identity(session)
            })
            .await?;
    }

    let resolved = handle
        .launch()
        .resolve_name_recipient(project_id, "observer-1", sender)
        .await?;
    assert!(matches!(resolved, SessionIdResolution::Resolved(uuid) if uuid == named));
    let self_lookup = handle
        .launch()
        .resolve_name_recipient(project_id, "observer-1", named)
        .await?;
    assert!(matches!(self_lookup, SessionIdResolution::NotFound));

    let SessionIdResolution::Ambiguous(mut uuids) = handle
        .launch()
        .resolve_name_recipient(project_id, "worker", sender)
        .await?
    else {
        anyhow::bail!("two sessions named worker must resolve as ambiguous");
    };
    uuids.sort();
    let mut expected = vec![dup_a, dup_b];
    expected.sort();
    assert_eq!(uuids, expected);
    let missing = handle
        .launch()
        .resolve_name_recipient(project_id, "nobody-here", sender)
        .await?;
    assert!(matches!(missing, SessionIdResolution::NotFound));
    Ok(())
}
