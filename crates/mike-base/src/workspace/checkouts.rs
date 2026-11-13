use std::path::Path;

use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::git_repo::{RepoIdentity, repo_identity};

pub async fn project_for_repo(
    persist: &PersistHandle,
    machine: Uuid,
    identity: &RepoIdentity,
) -> Result<Option<i64>, PersistError> {
    let main_root = identity.main_root.to_string_lossy();
    let roots = persist.machine_location_roots(machine).await?;
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
    match persist.project_ids_for_origin(origin).await?.as_slice() {
        [project] => Ok(Some(*project)),
        _ => Ok(None),
    }
}

pub async fn project_or_new(
    persist: &PersistHandle,
    machine: Uuid,
    identity: &RepoIdentity,
) -> Result<Option<i64>, PersistError> {
    if let Some(project) = project_for_repo(persist, machine, identity).await? {
        return Ok(Some(project));
    }
    let Some(origin) = identity.origin_url.as_deref() else {
        return Ok(None);
    };
    let scratch = is_scratch(&identity.main_root.to_string_lossy());
    if scratch || !persist.project_ids_for_origin(origin).await?.is_empty() {
        return Ok(None);
    }
    let name = origin.rsplit('/').next().unwrap_or(origin);
    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, origin.as_bytes());
    let project = persist.define_knowledge_project(name, uuid).await?;
    Ok(Some(project.id))
}

pub async fn place_folder(
    persist: &PersistHandle,
    machine: Uuid,
    machine_name: &str,
    path: &str,
) -> Result<Option<i64>, PersistError> {
    let Some(identity) = repo_identity(Path::new(path)) else {
        return persist.project_id_enclosing(machine, path).await;
    };
    let Some(project) = project_or_new(persist, machine, &identity).await? else {
        return Ok(None);
    };
    let root = identity.checkout_root.to_string_lossy();
    let location = persist
        .add_checkout_location(machine, machine_name, &root, project)
        .await?;
    if let (Some(location), Some(origin)) = (location, identity.origin_url.as_deref()) {
        persist.set_location_origin(location, origin).await?;
    }
    Ok(Some(project))
}

pub async fn record_origins(persist: &PersistHandle, machine: Uuid) -> Result<(), PersistError> {
    for location in persist.locations_on_machine(machine).await? {
        let root = Path::new(location.root_path.trim_end_matches('/'));
        let Some(identity) = repo_identity(root) else {
            continue;
        };
        if identity.checkout_root != root {
            continue;
        }
        if let Some(origin) = identity.origin_url.as_deref() {
            persist
                .set_location_origin(location.location_uuid, origin)
                .await?;
        }
    }
    Ok(())
}

const SCRATCH_PREFIXES: [&str; 5] = [
    "/tmp/",
    "/var/folders/",
    "/private/tmp/",
    "/private/var/folders/",
    "__",
];
const SCRATCH_PARTS: [&str; 3] = [
    "/.claude/.cache/tmp/",
    "/.local/state/mike/seats/",
    "{parent}",
];
pub fn is_scratch(path: &str) -> bool {
    SCRATCH_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || SCRATCH_PARTS.iter().any(|part| path.contains(part))
}
