use std::path::Path;

use uuid::Uuid;

use crate::launch::state::LaunchState;
use crate::store::persistence::project_identity::ProjectIdentity;
use crate::store::persistence::session_parent::scratchpad_parent_session;
use crate::store::persistence::workspace::RegisteredWorkspaceSession;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::git_repo::repo_identity_bounded;
use crate::workspace::home_folder::bind_in_machine_home;

pub(super) async fn resolve_from_cwd(
    state: &LaunchState,
    handle: &PersistHandle,
    registered: &mut RegisteredWorkspaceSession,
    session: Uuid,
    cwd: &str,
) {
    match cwd_project(state, handle, session, cwd).await {
        Ok(Some(project)) => {
            state
                .session_projects
                .note(session.to_string(), project.project_path.clone());
            registered.project_id = project.id;
            registered.project_path = Some(project.project_path);
            registered.project_resolution = "operator".to_string();
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(
            session_id = %session,
            cwd,
            %error,
            "could not resolve the registered session's project from its cwd"
        ),
    }
}

async fn cwd_project(
    state: &LaunchState,
    handle: &PersistHandle,
    session: Uuid,
    cwd: &str,
) -> Result<Option<ProjectIdentity>, PersistError> {
    if let Some(parent) = scratchpad_parent_session(cwd)
        && let Some(project) = handle.adopt_parent_session_project(session, parent).await?
    {
        return Ok(Some(project));
    }
    let Some(machine) = state.resolve_host_machine_uuid().await else {
        return Ok(None);
    };
    let identity = repo_identity_bounded(Path::new(cwd)).await;
    let floor = identity
        .as_ref()
        .map(|identity| identity.main_root.to_string_lossy().into_owned());
    let bound = handle
        .bind_session_to_local_root_within(session, machine, cwd, floor.as_deref())
        .await?;
    if bound.is_some() || identity.is_some() {
        return Ok(bound);
    }
    bind_in_machine_home(handle, session, machine, cwd).await
}
