use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::path::Path;
use std::sync::Arc;

use uuid::Uuid;

use crate::state::AppState;
use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::project_knowledge::ProjectIdentity;
use crate::store::persistence::session_parent::scratchpad_parent_session;
use crate::store::persistence::workspace::RegisteredWorkspaceSession;
use crate::workspace::git_repo::repo_identity;
use crate::workspace::home_folder::bind_in_machine_home;

pub async fn bind_host_location(
    state: &AppState,
    persist: &PersistHandle,
    session: Uuid,
    cwd: &str,
    registered: &mut RegisteredWorkspaceSession,
) {
    let Some(machine) = state.host_machine_uuid.get().copied() else {
        return;
    };
    let bound = match adopt_scratchpad_parent(persist, session, cwd).await {
        Ok(None) => bind_checkout(persist, session, machine, cwd).await,
        other => other,
    };
    let bound = match bound {
        Ok(None) => bind_in_machine_home(persist, session, machine, cwd).await,
        other => other,
    };
    match bound {
        Ok(Some(project)) => apply_project_binding(state, session, cwd, registered, project),
        Ok(None) => {}
        Err(error) => log_bind_failure(session, cwd, &error),
    }
}

async fn bind_checkout(
    persist: &PersistHandle,
    session: Uuid,
    machine: Uuid,
    cwd: &str,
) -> Result<Option<ProjectIdentity>, PersistError> {
    let repo_root = repo_identity(Path::new(cwd))
        .map(|identity| identity.main_root.to_string_lossy().into_owned());
    let knowledge = persist.knowledge();
    match knowledge
        .bind_session_to_local_root_within(session, machine, cwd, repo_root.as_deref())
        .await
    {
        Ok(None) => match super::workspace_repo::bind_by_repo(persist, session, machine, cwd).await
        {
            Ok(None) if repo_root.is_some() => {
                knowledge
                    .bind_session_to_local_root(session, machine, cwd)
                    .await
            }
            other => other,
        },
        other => other,
    }
}

pub async fn adopt_scratchpad_parent(
    persist: &PersistHandle,
    session: Uuid,
    cwd: &str,
) -> Result<Option<ProjectIdentity>, PersistError> {
    match scratchpad_parent_session(cwd) {
        Some(parent) => persist.adopt_parent_session_project(session, parent).await,
        None => Ok(None),
    }
}

fn apply_project_binding(
    state: &AppState,
    session: Uuid,
    cwd: &str,
    registered: &mut RegisteredWorkspaceSession,
    project: ProjectIdentity,
) {
    state.lesson_cache.rcu(|current| {
        Arc::new(current.with_session_project(session.to_string(), project.project_path.clone()))
    });
    tracing::info!(
        session_id = %session,
        project_uuid = %project.project_uuid,
        cwd,
        "bound session to its knowledge location from the first captured request"
    );
    registered.project_id = project.id;
    registered.project_path = Some(project.project_path);
    registered.project_resolution = "operator".to_string();
}

fn log_bind_failure(session: Uuid, cwd: &str, error: &PersistError) {
    tracing::warn!(
        session_id = %session,
        cwd,
        %error,
        "could not bind session to a knowledge location from the first captured request"
    );
}
