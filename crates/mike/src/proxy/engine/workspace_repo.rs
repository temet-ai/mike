use std::path::Path;

use uuid::Uuid;

use crate::store::persistence::project_knowledge::{ProjectIdentity, RepoLocationAdoption};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::git_repo::{RepoIdentity, repo_identity};

pub(super) async fn bind_by_repo(
    persist: &PersistHandle,
    session: Uuid,
    machine: Uuid,
    cwd: &str,
) -> Result<Option<ProjectIdentity>, PersistError> {
    let Some(identity) = repo_identity(Path::new(cwd)) else {
        return Ok(None);
    };
    let Some(project_id) = project_for_repo(persist, machine, &identity).await? else {
        return Ok(None);
    };
    let Ok(machine_identity) = crate::machine_identity::load_default().await else {
        return Ok(None);
    };
    let checkout_root = identity.checkout_root.to_string_lossy();
    let adoption = RepoLocationAdoption {
        session,
        machine,
        machine_name: &machine_identity.machine_name,
        checkout_root: &checkout_root,
        project_id,
    };
    let Some(location) = persist.adopt_repo_location(&adoption).await? else {
        return Ok(None);
    };
    persist
        .bind_session_location(session, location)
        .await
        .map(Some)
}

async fn project_for_repo(
    persist: &PersistHandle,
    machine: Uuid,
    identity: &RepoIdentity,
) -> Result<Option<i64>, PersistError> {
    let roots = persist.machine_location_roots(machine).await?;
    let main_root = identity.main_root.to_string_lossy();
    if let Some(location) = roots
        .iter()
        .find(|location| location.root_path.trim_end_matches('/') == main_root)
    {
        return Ok(Some(location.project_id));
    }
    if let Some(project) = persist.project_id_for_path(&main_root).await? {
        return Ok(Some(project));
    }
    let Some(origin) = identity.origin_url.as_deref() else {
        return Ok(None);
    };
    Ok(roots.iter().find_map(|location| {
        let other = repo_identity(Path::new(&location.root_path))?;
        (other.origin_url.as_deref() == Some(origin)).then_some(location.project_id)
    }))
}
