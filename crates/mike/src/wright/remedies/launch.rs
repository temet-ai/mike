use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Halt, Owners};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::remedies::{LaunchState, PendingAction};

fn launch_id(item: &PendingAction) -> Result<i64, Halt> {
    item.resource.trim().parse().map_err(|_| {
        Halt::Refused(format!(
            "problem {} names resource `{}`, which is not a launch id",
            item.problem_id, item.resource
        ))
    })
}

async fn read(owners: &Owners, id: i64) -> Result<LaunchState, Halt> {
    owners
        .persist
        .web()
        .wright_launch_state(id)
        .await?
        .ok_or_else(|| Halt::Refused(format!("launch {id} does not exist")))
}

pub(crate) async fn seat(owners: &Owners, item: &PendingAction) -> Result<Option<Uuid>, Halt> {
    Ok(read(owners, launch_id(item)?).await?.session_uuid)
}

pub(crate) async fn prior(owners: &Owners, item: &PendingAction) -> Result<Value, Halt> {
    let launch = read(owners, launch_id(item)?).await?;
    if let Some(closed_at) = launch.closed_at {
        return Err(Halt::Refused(format!(
            "launch {} was already closed at {closed_at}",
            launch.id
        )));
    }
    Ok(json!({
        "launch_id": launch.id,
        "session_uuid": launch.session_uuid,
        "run_id": launch.run_id,
        "seat": launch.seat,
        "closed_at": null,
        "close_reason": launch.close_reason,
    }))
}

pub(crate) fn close_reason(action_id: i64) -> String {
    format!(
        "wright action {action_id}: the process was reaped and the session made no request since"
    )
}

pub(crate) async fn apply(
    owners: &Owners,
    item: &PendingAction,
    action_id: i64,
) -> Result<Value, Halt> {
    let id = launch_id(item)?;
    match owners
        .persist
        .launch()
        .record_launch_closed(id, &close_reason(action_id), Some(true))
        .await
    {
        Ok(()) => Ok(json!({"outcome": "closed", "launch_id": id})),
        Err(PersistError::DataInvariant(reason)) => Err(Halt::Refused(reason)),
        Err(e) => Err(e.into()),
    }
}

pub(crate) async fn verify(owners: &Owners, item: &PendingAction) -> Result<Value, Halt> {
    let launch = read(owners, launch_id(item)?).await?;
    let Some(closed_at) = launch.closed_at else {
        return Err(Halt::Failed(format!(
            "launch {} is still open when read again after the close",
            launch.id
        )));
    };
    Ok(json!({
        "outcome": "closed",
        "launch_id": launch.id,
        "closed_at": closed_at,
        "close_reason": launch.close_reason,
    }))
}
