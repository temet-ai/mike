use std::time::{Duration, Instant};

use uuid::Uuid;

use super::maybe_register_workspace_session;
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};
use crate::store::persistence::workspace::{RegisterWorkspaceSession, WorkspaceRosterRow};
use crate::store::persistence::{PersistHandle, SENTINEL_PROJECT_PATH};
use crate::workspace::RosterScope;

struct Env {
    db: TestDbHandle,
    state: AppState,
    persist: PersistHandle,
}

impl Env {
    async fn boot() -> anyhow::Result<Option<Self>> {
        let Some(db) = acquire_ephemeral_db().await? else {
            return Ok(None);
        };
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            &db.database_url,
        ])?;
        let state = AppState::new(config).await?;
        let persist = state
            .persist()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("persist_enabled config must yield a PersistHandle"))?;
        Ok(Some(Self { db, state, persist }))
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }
}

async fn roster_rows_for_session(
    persist: &PersistHandle,
    project_path: &str,
    session_uuid: Uuid,
) -> anyhow::Result<Vec<WorkspaceRosterRow>> {
    let Some(project_id) = persist.find_project_id_by_path(project_path).await? else {
        return Ok(Vec::new());
    };
    let roster = persist
        .fetch_workspace_roster(project_id, RosterScope::All)
        .await?;
    Ok(roster
        .into_iter()
        .filter(|row| row.session_uuid == session_uuid)
        .collect())
}

#[tokio::test]
async fn capture_path_registers_a_fresh_session_on_the_roster() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };

    let project_path = format!("/tmp/workspace-capture-test-{}", Uuid::new_v4());
    let session_uuid = Uuid::new_v4();

    maybe_register_workspace_session(&env.state, &session_uuid.to_string(), &project_path, true);

    let start = Instant::now();
    let row = loop {
        let rows =
            roster_rows_for_session(&env.persist, SENTINEL_PROJECT_PATH, session_uuid).await?;
        assert!(
            rows.len() <= 1,
            "a fresh session must land at most one roster row, found {}",
            rows.len()
        );
        if let Some(row) = rows.into_iter().next() {
            break row;
        }
        if start.elapsed() > Duration::from_secs(5) {
            anyhow::bail!(
                "timed out after {:?} waiting for the capture-path registration to land",
                start.elapsed()
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(row.registered_via, "passive");
    let minted_from_marker = env.persist.find_project_id_by_path(&project_path).await?;
    assert!(
        minted_from_marker.is_none(),
        "capture-path registration must never mint a project from the captured project_path"
    );

    env.cleanup().await
}

#[tokio::test]
async fn capture_path_does_not_duplicate_an_already_hook_registered_session() -> anyhow::Result<()>
{
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };

    let project_path = format!("/tmp/workspace-capture-test-{}", Uuid::new_v4());
    let session_uuid = Uuid::new_v4();

    env.persist
        .register_workspace_session(RegisterWorkspaceSession {
            session_uuid,
            project_path: Some(&project_path),
            cwd: &project_path,
            worktree: None,
            registered_via: "hook",
            intent: None,
            role: None,
            name: None,
            instructions: None,
            pid: None,
        })
        .await?;

    maybe_register_workspace_session(&env.state, &session_uuid.to_string(), &project_path, true);

    let start = Instant::now();
    loop {
        let rows = roster_rows_for_session(&env.persist, &project_path, session_uuid).await?;
        assert!(
            rows.len() <= 1,
            "capture-path registration must upsert an existing session, never duplicate it: found {} rows",
            rows.len()
        );
        if !rows.is_empty() {
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            anyhow::bail!(
                "timed out after {:?} waiting for the hook-registered row to still be present",
                start.elapsed()
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    env.cleanup().await
}

#[tokio::test]
async fn capture_path_registers_the_worktree_name_from_project_path() -> anyhow::Result<()> {
    let Some(env) = Env::boot().await? else {
        return Ok(());
    };

    let worktree_name = format!("wt-{}", Uuid::new_v4());
    let project_path = format!(
        "/tmp/workspace-capture-test-{}/.claude/worktrees/{worktree_name}",
        Uuid::new_v4()
    );
    let session_uuid = Uuid::new_v4();

    maybe_register_workspace_session(&env.state, &session_uuid.to_string(), &project_path, true);

    let start = Instant::now();
    let row = loop {
        let rows =
            roster_rows_for_session(&env.persist, SENTINEL_PROJECT_PATH, session_uuid).await?;
        if let Some(row) = rows.into_iter().next() {
            break row;
        }
        if start.elapsed() > Duration::from_secs(5) {
            anyhow::bail!(
                "timed out after {:?} waiting for the capture-path registration to land",
                start.elapsed()
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(row.worktree.as_deref(), Some(worktree_name.as_str()));

    env.cleanup().await
}
