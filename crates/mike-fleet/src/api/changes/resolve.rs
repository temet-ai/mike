use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::path::Path;

use uuid::Uuid;

use super::{ChangesError, ChangesQuery};
use crate::change_record::SessionIdentity;
use crate::change_record::answers::Caller;
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::commit::{owner_of, ownership};
use crate::launch::fleet::seated::Seated;
use crate::launch::seat_fact::is_fleet_seat;
use crate::store::persistence::PersistHandle;
use crate::workspace::git_repo::repo_identity;

pub struct FleetSeat {
    pub definition: FleetDefinition,
    pub run: String,
    pub task: String,
    pub seat: String,
    pub files: Vec<String>,
}

pub struct Resolved {
    pub identity: Option<SessionIdentity>,
    pub checkout: String,
    pub fleet: Option<FleetSeat>,
}

impl Resolved {
    pub fn caller(&self) -> Result<Caller, ChangesError> {
        let identity = self.identity.as_ref().ok_or_else(|| {
            ChangesError::Invalid("this view needs `session`, your own session id".to_owned())
        })?;
        Ok(Caller {
            session_id: identity.id,
            checkout: self.checkout.clone(),
            in_fleet: self.fleet.is_some(),
            task_files: self
                .fleet
                .as_ref()
                .map(|f| f.files.clone())
                .unwrap_or_default(),
        })
    }

    pub fn relative(&self, path: &str) -> String {
        let root = format!("{}/", self.checkout.trim_end_matches('/'));
        path.strip_prefix(&root).unwrap_or(path).to_owned()
    }

    pub fn owner_of(&self, relative: &str) -> Option<(String, String)> {
        let fleet = self.fleet.as_ref()?;
        owner_of(&fleet.definition, relative).map(|(task, seat)| (task.clone(), seat.to_owned()))
    }
}

pub async fn resolve(
    persist: &PersistHandle,
    query: &ChangesQuery,
) -> Result<Resolved, ChangesError> {
    let identity = match query.session.as_deref().map(str::trim) {
        Some(text) if !text.is_empty() => {
            let uuid = Uuid::parse_str(text).map_err(|e| {
                ChangesError::Invalid(format!("`session` {text:?} is not a session id: {e}"))
            })?;
            Some(
                persist
                    .session_identity_by_uuid(uuid)
                    .await?
                    .ok_or_else(|| ChangesError::UnknownSession(text.to_owned()))?,
            )
        }
        _ => None,
    };
    let fleet = match &identity {
        Some(identity) => seat_of(persist, identity.uuid).await?,
        None => None,
    };
    let checkout = checkout_of(query, identity.as_ref(), fleet.as_ref())?;
    Ok(Resolved {
        identity,
        checkout,
        fleet,
    })
}

async fn seat_of(
    persist: &PersistHandle,
    session: Uuid,
) -> Result<Option<FleetSeat>, ChangesError> {
    if !is_fleet_seat(persist, session).await {
        return Ok(None);
    }
    let Some(row) = persist.fleet().find_fleet_for_session(session).await? else {
        return Ok(None);
    };
    let (Some(run), "running") = (row.run_id.clone(), row.state.as_str()) else {
        return Ok(None);
    };
    let Ok(definition) = FleetDefinition::from_value(&row.definition) else {
        return Ok(None);
    };
    let seated = Seated::of_run(persist, &run, &definition).await;
    let Some(owner) = ownership(&definition, &seated, session) else {
        return Ok(None);
    };
    Ok(Some(FleetSeat {
        run,
        task: owner.task,
        seat: owner.seat,
        files: owner.owned,
        definition,
    }))
}

fn checkout_of(
    query: &ChangesQuery,
    identity: Option<&SessionIdentity>,
    fleet: Option<&FleetSeat>,
) -> Result<String, ChangesError> {
    if let Some(seat) = fleet {
        return Ok(seat.definition.repo.clone());
    }
    if let Some(named) = query
        .checkout
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        return Ok(named.trim_end_matches('/').to_owned());
    }
    let cwd = identity.and_then(|i| i.cwd.as_deref());
    cwd.and_then(|dir| repo_identity(Path::new(dir)))
        .map(|repo| repo.checkout_root.to_string_lossy().into_owned())
        .ok_or_else(|| {
            ChangesError::Invalid(format!(
                "no checkout: pass `checkout`, or ask from a session whose working folder is inside a git repository (this session's working folder: {:?})",
                cwd.unwrap_or("none recorded")
            ))
        })
}
