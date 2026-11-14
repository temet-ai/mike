use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::Value;

use super::{Change, Conversation, SteerError};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::conversation_steers::SteerRow;

const PENDING_BATCH: i64 = 20;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum UndoError {
    #[error("no steer has id {0}")]
    NotFound(i64),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Failed(String),
}

impl From<PersistError> for UndoError {
    fn from(e: PersistError) -> Self {
        UndoError::Failed(format!("the database refused the undo: {e}"))
    }
}

fn stored_change(row: &SteerRow) -> Result<Change, String> {
    serde_json::from_value(row.change.clone()).map_err(|e| {
        format!(
            "steer {} holds `{}`, which is not a known change: {e}",
            row.id, row.change
        )
    })
}

async fn apply_one(convo: &Conversation, row: &SteerRow) -> Result<bool, PersistError> {
    let web = convo.persist.web();
    let change = match stored_change(row) {
        Ok(change) => change,
        Err(reason) => return web.refuse_wright_steer(row.id, &reason).await,
    };
    match convo.steers.apply(row.id, &change).await {
        Ok(effect) => web.mark_wright_steer_applied(row.id, &effect).await,
        Err(e) => {
            tracing::info!(steer = row.id, reason = %e, "wright: a steer was not applied");
            web.refuse_wright_steer(row.id, &e.to_string()).await
        }
    }
}

pub(crate) async fn apply_pending(convo: &Conversation) -> Result<usize, PersistError> {
    let pending = convo
        .persist
        .web()
        .pending_wright_steers(PENDING_BATCH)
        .await?;
    let mut applied = 0;
    for row in &pending {
        if apply_one(convo, row).await? {
            applied += 1;
        }
    }
    Ok(applied)
}

fn undoable(row: &SteerRow, version: i32) -> Result<(Value, Value), UndoError> {
    if row.version != version {
        return Err(UndoError::Conflict(format!(
            "steer {} is at version {}, and the undo was for version {version}; reload the conversation",
            row.id, row.version
        )));
    }
    if let Some(refusal) = &row.refusal {
        return Err(UndoError::Conflict(format!(
            "steer {} was refused ({refusal}), so nothing changed and there is nothing to undo",
            row.id
        )));
    }
    if let Some(at) = row.undone_at {
        return Err(UndoError::Conflict(format!(
            "steer {} was already undone at {}",
            row.id,
            at.to_rfc3339()
        )));
    }
    match (&row.applied_at, &row.prior, &row.effect) {
        (Some(_), Some(prior), Some(effect)) => Ok((prior.clone(), effect.clone())),
        _ => Err(UndoError::Conflict(format!(
            "steer {} has not been applied yet; undo it once it shows as applied",
            row.id
        ))),
    }
}

pub(crate) async fn undo(
    convo: &Conversation,
    id: i64,
    version: i32,
) -> Result<SteerRow, UndoError> {
    let web = convo.persist.web();
    let row = web.wright_steer(id).await?.ok_or(UndoError::NotFound(id))?;
    let (prior, effect) = undoable(&row, version)?;
    let change = stored_change(&row).map_err(UndoError::Failed)?;
    match convo.steers.undo(&change, &prior, &effect).await {
        Ok(_) => {}
        Err(SteerError::Conflict(reason) | SteerError::Refused(reason)) => {
            return Err(UndoError::Conflict(reason));
        }
        Err(SteerError::Failed(reason)) => return Err(UndoError::Failed(reason)),
    }
    web.mark_wright_steer_undone(id, version)
        .await?
        .ok_or_else(|| {
            UndoError::Conflict(format!(
                "steer {id} changed while it was being undone; reload the conversation"
            ))
        })
}
