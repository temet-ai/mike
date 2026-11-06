use axum::http::StatusCode;
use axum::response::Response;
use uuid::Uuid;

use super::{database, problem};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::mailbox::{FleetTarget, SessionIdResolution, TargetError};

pub(super) const REFERENCE: &str = "reference";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Destination {
    pub session: Uuid,
    pub task: Option<String>,
}

pub(super) async fn session(
    persist: &PersistHandle,
    given: &str,
    what: &str,
) -> Result<Uuid, Box<Response>> {
    let given = given.trim();
    if let Ok(uuid) = Uuid::parse_str(given) {
        return Ok(uuid);
    }
    if given.len() < 8 {
        return Err(Box::new(problem(
            StatusCode::BAD_REQUEST,
            format!(
                "{what} `{given}` is not a session id; give the full id or a prefix of 8 or more characters"
            ),
        )));
    }
    match persist.resolve_session_id_prefix(given).await {
        Ok(SessionIdResolution::Resolved(uuid)) => Ok(uuid),
        Ok(SessionIdResolution::NotFound) => Err(Box::new(problem(
            StatusCode::NOT_FOUND,
            format!("no session starts with `{given}` ({what})"),
        ))),
        Ok(SessionIdResolution::Ambiguous(found)) => Err(Box::new(problem(
            StatusCode::BAD_REQUEST,
            format!(
                "`{given}` ({what}) starts {} sessions; give more of the id",
                found.len()
            ),
        ))),
        Err(err) => Err(Box::new(database(err))),
    }
}

async fn reference_seat(
    persist: &PersistHandle,
    asker: Uuid,
) -> Result<Destination, Box<Response>> {
    let definition = persist
        .sender_fleet_definition(asker)
        .await
        .map_err(|e| Box::new(database(e)))?;
    let reference = definition
        .filter(|d| d.reference.enabled)
        .and_then(|d| d.reference.session)
        .and_then(|s| Uuid::parse_str(s.trim()).ok());
    reference
        .map(|session| Destination {
            session,
            task: None,
        })
        .ok_or_else(|| {
            Box::new(problem(
                StatusCode::BAD_REQUEST,
                "`reference` needs the asker to be a seat of a running fleet that has a reference seat",
            ))
        })
}

fn refused(err: TargetError) -> Box<Response> {
    match err {
        TargetError::Persist(e) => Box::new(database(e)),
        other => Box::new(problem(StatusCode::BAD_REQUEST, other.to_string())),
    }
}

pub(super) async fn destination(
    persist: &PersistHandle,
    asker: Uuid,
    to: &str,
) -> Result<Destination, Box<Response>> {
    let to = to.trim();
    if to == REFERENCE {
        return reference_seat(persist, asker).await;
    }
    match persist.resolve_fleet_target(asker, to).await {
        Ok(Some(FleetTarget::Seat { session, task })) => Ok(Destination { session, task }),
        Ok(Some(FleetTarget::Seats(_))) => Err(Box::new(problem(
            StatusCode::BAD_REQUEST,
            "a question goes to one seat: give a session id, task:<id>, owner:<path> or reference",
        ))),
        Ok(None) => session(persist, to, "to").await.map(|session| Destination {
            session,
            task: None,
        }),
        Err(err) => Err(refused(err)),
    }
}
