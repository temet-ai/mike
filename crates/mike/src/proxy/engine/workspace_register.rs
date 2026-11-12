use std::sync::Arc;

use dashmap::mapref::entry::Entry;
use tokio::sync::OnceCell;

use crate::api::workspace::extract_worktree_name;
use crate::state::AppState;
use crate::store::persistence::workspace::{RegisterWorkspaceSession, RegisteredWorkspaceSession};

pub(crate) fn maybe_register_workspace_session(
    state: &AppState,
    session_id: &str,
    project_path: &str,
    enabled: bool,
) {
    if !enabled {
        return;
    }
    if state.persist().is_none() {
        return;
    }
    if project_path.is_empty() {
        return;
    }
    if !claim_new_cell(state, session_id) {
        return;
    }

    let state = state.clone();
    let session_id = session_id.to_owned();
    let project_path = project_path.to_owned();
    let tracker = state.tracker.clone();
    crate::observability::spans::spawn_tracked(&tracker, async move {
        state.resolve_host_machine_uuid().await;
        register_via_cell(&state, &session_id, &project_path).await;
    });
}

pub(crate) async fn await_workspace_registration(
    state: &AppState,
    session_id: &str,
    project_path: &str,
) -> Option<RegisteredWorkspaceSession> {
    register_via_cell(state, session_id, project_path).await
}

fn claim_new_cell(state: &AppState, session_id: &str) -> bool {
    match state
        .workspace_registration_cells
        .entry(session_id.to_owned())
    {
        Entry::Occupied(_) => false,
        Entry::Vacant(v) => {
            v.insert(Arc::new(OnceCell::new()));
            true
        }
    }
}

async fn register_via_cell(
    state: &AppState,
    session_id: &str,
    project_path: &str,
) -> Option<RegisteredWorkspaceSession> {
    let persist = state.persist()?;
    let session_uuid = uuid::Uuid::parse_str(session_id).ok()?;
    let cell = state
        .workspace_registration_cells
        .entry(session_id.to_owned())
        .or_insert_with(|| Arc::new(OnceCell::new()))
        .clone();
    let worktree = extract_worktree_name(project_path);

    let result = cell
        .get_or_try_init(|| async {
            let mut registered = persist
                .register_workspace_session(RegisterWorkspaceSession {
                    session_uuid,
                    project_path: None,
                    cwd: project_path,
                    worktree: worktree.as_deref(),
                    registered_via: "passive",
                    intent: None,
                    role: None,
                    name: None,
                    instructions: None,
                    pid: None,
                })
                .await?;
            super::workspace_location::bind_host_location(
                state,
                persist,
                session_uuid,
                project_path,
                &mut registered,
            )
            .await;
            Ok::<_, crate::store::persistence::PersistError>(registered)
        })
        .await;

    match result {
        Ok(registered) => Some(registered.clone()),
        Err(err) => {
            tracing::warn!(
                session_id,
                error = %err,
                "failed to register session on workspace roster from capture path"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::maybe_register_workspace_session;
    use crate::config::ProxyConfig;
    use crate::state::AppState;

    async fn in_memory_state() -> anyhow::Result<AppState> {
        AppState::new(ProxyConfig::for_tests_in_memory()).await
    }

    #[tokio::test]
    async fn disabled_toggle_never_attempts_or_memoizes() -> anyhow::Result<()> {
        let state = in_memory_state().await?;
        maybe_register_workspace_session(&state, "s1", "/tmp/proj", false);
        assert!(
            !state.workspace_registration_cells.contains_key("s1"),
            "a disabled toggle must not even record an attempt"
        );
        Ok(())
    }

    #[tokio::test]
    async fn no_persist_handle_never_memoizes() -> anyhow::Result<()> {
        let state = in_memory_state().await?;
        maybe_register_workspace_session(&state, "s1", "/tmp/proj", true);
        assert!(!state.workspace_registration_cells.contains_key("s1"));
        Ok(())
    }

    #[tokio::test]
    async fn empty_project_path_is_skipped() -> anyhow::Result<()> {
        let state = in_memory_state().await?;
        maybe_register_workspace_session(&state, "s1", "", true);
        assert!(
            !state.workspace_registration_cells.contains_key("s1"),
            "no resolved project means nothing was attempted"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "workspace_register_db_tests.rs"]
mod db_tests;
