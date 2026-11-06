use std::collections::BTreeSet;

use uuid::Uuid;

use super::author::{Authorship, Window};
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::checks::normal;
use crate::launch::fleet::commit::covers;
use crate::launch::seat_fact::is_fleet_seat;
use crate::store::persistence::change_record::NamedShellCall;
use crate::store::persistence::{PersistError, PersistHandle};

pub fn from_named_calls(calls: &[NamedShellCall]) -> Option<Authorship> {
    let sessions: BTreeSet<i64> = calls.iter().map(|call| call.session_id).collect();
    let [session] = sessions.into_iter().collect::<Vec<_>>()[..] else {
        return None;
    };
    let call = calls
        .iter()
        .filter(|call| call.session_id == session)
        .max_by_key(|call| (call.started, call.tool_use_id))?;
    Some(Authorship::Shell {
        session,
        named_by: Some(call.tool_use_id),
    })
}

pub fn owns(def: &FleetDefinition, session: Uuid, path: &str) -> bool {
    let wanted = normal(path);
    let session = session.to_string();
    def.tasks.values().any(|task| {
        task.session.as_deref().map(str::trim) == Some(session.as_str()) && {
            let owned: Vec<String> = task
                .files
                .iter()
                .chain(task.new_files.iter())
                .map(|file| normal(file))
                .collect();
            covers(&owned, &wanted)
        }
    })
}

pub fn by_owner(candidates: &[(i64, bool)]) -> Option<i64> {
    let owners: Vec<i64> = candidates
        .iter()
        .filter(|(_, owns)| *owns)
        .map(|(session, _)| *session)
        .collect();
    match owners[..] {
        [one] => Some(one),
        _ => None,
    }
}

pub async fn named_shell(
    persist: &PersistHandle,
    (root, relative, absolute): (&str, &str, &str),
    window: Window,
) -> Result<Option<Authorship>, PersistError> {
    let calls = persist
        .shell_calls_naming_path(root, (relative, absolute), window.from, window.to)
        .await?;
    Ok(from_named_calls(&calls))
}

pub async fn owner_among(
    persist: &PersistHandle,
    candidates: &[i64],
    relative: &str,
) -> Result<Option<i64>, PersistError> {
    let mut owning = Vec::with_capacity(candidates.len());
    for identity in persist.session_identities(candidates).await? {
        let def = if is_fleet_seat(persist, identity.uuid).await {
            persist
                .find_fleet_for_session(identity.uuid)
                .await?
                .and_then(|row| FleetDefinition::from_value(&row.definition).ok())
        } else {
            None
        };
        let owns = def.is_some_and(|def| owns(&def, identity.uuid, relative));
        owning.push((identity.id, owns));
    }
    Ok(by_owner(&owning))
}

#[cfg(test)]
#[path = "author_named_tests.rs"]
mod tests;
