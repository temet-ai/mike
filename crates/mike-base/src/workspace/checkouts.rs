use std::path::Path;

use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::git_repo::{RepoIdentity, repo_identity_bounded};

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
    let Some(identity) = repo_identity_bounded(Path::new(path)).await else {
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

pub async fn record_origins(
    persist: &PersistHandle,
    machine: Uuid,
) -> Result<Vec<crate::store::persistence::location_move::LocationMove>, PersistError> {
    for location in persist.locations_on_machine(machine).await? {
        if let Some(origin) = own_origin(&location.root_path).await
            && location.origin_url.as_deref() != Some(origin.as_str())
        {
            persist
                .set_location_origin(location.location_uuid, &origin)
                .await?;
        }
    }
    let moves = super::checkout_watch::watch_checkouts(persist, machine).await?;
    for moved in &moves {
        tracing::info!(location = %moved.location_uuid, from = %moved.from, to = %moved.to,
            "checkouts: a missing checkout reappeared at a new folder; its location moved there");
    }
    Ok(moves)
}

async fn own_origin(root_path: &str) -> Option<String> {
    let root = Path::new(root_path.trim_end_matches('/'));
    let identity = repo_identity_bounded(root).await?;
    (identity.checkout_root == root)
        .then_some(identity.origin_url)
        .flatten()
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
