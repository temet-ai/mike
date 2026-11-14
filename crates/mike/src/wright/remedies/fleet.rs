use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Halt, Owners};
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::plan::ControlSeats;
use crate::store::persistence::wright::remedies::PendingAction;

const LIVE_STATES: [&str; 2] = ["starting", "running"];
pub(crate) const TOLD: &str = "told_coordinator";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunningFleet {
    pub(crate) run_id: String,
    pub(crate) coordinator: Option<Uuid>,
}

pub(crate) async fn running(owners: &Owners, seat: Uuid) -> Result<Option<RunningFleet>, Halt> {
    let Some(row) = owners.persist.fleet().find_fleet_for_session(seat).await? else {
        return Ok(None);
    };
    if !LIVE_STATES.contains(&row.state.as_str()) {
        return Ok(None);
    }
    let coordinator = FleetDefinition::from_value(&row.definition)
        .ok()
        .and_then(|def| ControlSeats::recorded(&def).coordinator);
    Ok(Some(RunningFleet {
        run_id: row
            .run_id
            .unwrap_or_else(|| format!("fleet {}", row.fleet_uuid)),
        coordinator,
    }))
}

pub(crate) fn message_uuid(action_id: i64) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("mike-wright-action-{action_id}").as_bytes(),
    )
}

pub(crate) fn notice(item: &PendingAction, fleet: &RunningFleet, seat: Uuid) -> String {
    format!(
        "[mike wright] Wright problem {problem} ({scope} {resource}) concerns seat session {seat} in fleet run {run}. \
         Wright's known fix `{remedy}` ({action}) would touch that seat, so Wright left it alone. \
         Decide whether the seat needs it.",
        problem = item.problem_id,
        scope = item.scope,
        resource = item.resource,
        run = fleet.run_id,
        remedy = item.remedy,
        action = item.action,
    )
}

pub(crate) async fn tell(
    owners: &Owners,
    item: &PendingAction,
    action_id: i64,
    fleet: &RunningFleet,
    seat: Uuid,
) -> Result<Value, Halt> {
    let Some(coordinator) = fleet.coordinator else {
        return Err(Halt::Refused(format!(
            "fleet run {} has no coordinator recorded; Wright leaves its seats alone",
            fleet.run_id
        )));
    };
    let message = message_uuid(action_id);
    let launch = owners.persist.launch();
    if launch.fetch_message_core_by_uuid(message).await?.is_none() {
        let sent = launch
            .insert_message(message, coordinator, None, &notice(item, fleet, seat))
            .await?;
        if sent.is_none() {
            return Err(Halt::Refused(format!(
                "the coordinator session {coordinator} of fleet run {} is not known to mike",
                fleet.run_id
            )));
        }
    }
    Ok(json!({
        "outcome": TOLD,
        "run_id": fleet.run_id,
        "coordinator": coordinator,
        "seat_session": seat,
        "message_uuid": message,
    }))
}

pub(crate) fn told(effect: &Value) -> bool {
    effect["outcome"].as_str() == Some(TOLD)
}

pub(crate) async fn verify(owners: &Owners, effect: &Value) -> Result<Value, Halt> {
    let message: Uuid = serde_json::from_value(effect["message_uuid"].clone())
        .map_err(|e| Halt::Failed(format!("the effect names no message ({e}): {effect}")))?;
    if owners
        .persist
        .launch()
        .fetch_message_core_by_uuid(message)
        .await?
        .is_none()
    {
        return Err(Halt::Failed(format!(
            "message {message} to the coordinator is not in the mailbox when read again"
        )));
    }
    Ok(effect.clone())
}
