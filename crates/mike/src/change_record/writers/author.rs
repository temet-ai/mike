use std::path::Path;

use uuid::Uuid;

use super::error::WriterError;
use crate::launch::seat_fact::is_fleet_seat;
use crate::store::persistence::PersistHandle;
use crate::workspace::git_repo::repo_identity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub session_id: i64,
    pub project_id: i64,
    pub checkout: String,
    pub run_id: Option<String>,
    pub task: Option<String>,
}

fn task_of(definition: &serde_json::Value, session: Uuid) -> Option<String> {
    let wanted = session.to_string();
    definition
        .get("tasks")?
        .as_object()?
        .iter()
        .find(|(_, task)| task.get("session").and_then(|s| s.as_str()) == Some(wanted.as_str()))
        .map(|(id, _)| id.clone())
}

pub async fn author_of(
    persist: &PersistHandle,
    session: Uuid,
) -> Result<Option<Author>, WriterError> {
    let Some(identity) = persist.session_identity_by_uuid(session).await? else {
        return Ok(None);
    };
    let Some(cwd) = identity.cwd.as_deref() else {
        return Ok(None);
    };
    let Some(repo) = repo_identity(Path::new(cwd)) else {
        return Ok(None);
    };
    let fleet = if is_fleet_seat(persist, session).await {
        persist.find_fleet_for_session(session).await?
    } else {
        None
    };
    let (run_id, task) = match fleet {
        Some(fleet) => {
            let task = task_of(&fleet.definition, session);
            (fleet.run_id, task)
        }
        None => (None, None),
    };
    Ok(Some(Author {
        session_id: identity.id,
        project_id: identity.project_id,
        checkout: repo.checkout_root.to_string_lossy().into_owned(),
        run_id,
        task,
    }))
}
