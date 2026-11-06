#[path = "migration.rs"]
mod migration;
#[path = "perf.rs"]
mod perf;
#[path = "races.rs"]
mod races;

use uuid::Uuid;

use mike::store::persistence::workspace::RegisterWorkspaceSession;

use crate::store::workspace::support::{shared_handle, unique};

fn input(session_uuid: Uuid, cwd: &str) -> RegisterWorkspaceSession<'_> {
    RegisterWorkspaceSession {
        session_uuid,
        project_path: None,
        cwd,
        worktree: None,
        registered_via: "hook",
        intent: None,
        role: None,
        name: None,
        instructions: None,
        pid: None,
    }
}

#[tokio::test]
async fn registration_creates_row_with_no_evidence_when_capture_never_saw_it() -> anyhow::Result<()>
{
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let cwd = unique("flow-admission-no-evidence");

    let result = handle
        .register_workspace_session(input(session, &cwd))
        .await?
        .flow_admission;
    assert!(
        result.evidence.is_none(),
        "a session registration alone must never itself admit — only capture does"
    );

    let retry = handle
        .register_workspace_session(input(session, &cwd))
        .await?
        .flow_admission;
    assert_eq!(retry.evidence, result.evidence);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn registration_reports_evidence_capture_already_admitted() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let cwd = unique("flow-admission-evidence");

    mike::store::persistence::id_resolvers::resolve_session_id(
        handle.pool(),
        &session.to_string(),
        None,
        Some(1),
    )
    .await?;

    let result = handle
        .register_workspace_session(input(session, &cwd))
        .await?
        .flow_admission;
    let evidence = result
        .evidence
        .ok_or_else(|| anyhow::anyhow!("capture-admitted row must carry evidence"))?;
    assert_eq!(evidence.capture_version, 1);
    assert_eq!(evidence.producer, "mike");
    assert_eq!(evidence.hook_event_name, "capture");
    assert_eq!(evidence.source, "capture");

    let retry = handle
        .register_workspace_session(input(session, &cwd))
        .await?
        .flow_admission;
    assert_eq!(retry.evidence, Some(evidence));

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn registration_failure_rolls_back_new_session_row() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let cwd = unique("flow-admission-rollback");
    let mut invalid = input(session, &cwd);
    invalid.cwd = "invalid\0postgres-text";
    assert!(handle.register_workspace_session(invalid).await.is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sessions.sessions WHERE session_uuid = $1")
            .bind(session)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(count, 0);
    let retry = handle
        .register_workspace_session(input(session, &cwd))
        .await?
        .flow_admission;
    assert!(retry.evidence.is_none());
    handle.pool().close().await;
    Ok(())
}
