use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::path::Path;

use uuid::Uuid;

use crate::store::persistence::project_knowledge::{ProjectIdentity, RepoLocationAdoption};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::checkouts::project_or_new;
use crate::workspace::git_repo::repo_identity;

pub async fn bind_by_repo(
    persist: &PersistHandle,
    session: Uuid,
    machine: Uuid,
    cwd: &str,
) -> Result<Option<ProjectIdentity>, PersistError> {
    let Some(identity) = repo_identity(Path::new(cwd)) else {
        return Ok(None);
    };
    let Some(project_id) = project_or_new(persist, machine, &identity).await? else {
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
    let Some(location) = persist.knowledge().adopt_repo_location(&adoption).await? else {
        return Ok(None);
    };
    if let Some(origin) = identity.origin_url.as_deref() {
        persist
            .knowledge()
            .set_location_origin(location, origin)
            .await?;
    }
    persist
        .knowledge()
        .bind_session_location(session, location)
        .await
        .map(Some)
}
