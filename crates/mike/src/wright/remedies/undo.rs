use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::Value;

use super::{Action, Halt, Owners, filter_rule, fleet, setting};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::remedies::ActionRow;

const UNDOABLE_STATES: [&str; 2] = ["verified", "ineffective"];

#[derive(Debug, thiserror::Error)]
pub(crate) enum UndoError {
    #[error("no Wright action has id {0}")]
    NotFound(i64),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error(transparent)]
    Persist(#[from] PersistError),
}

pub(crate) fn why_not(row: &ActionRow) -> Option<String> {
    let Some(action) = Action::from_code(&row.action).filter(|a| a.can_undo()) else {
        return Some(format!(
            "action {} ({}) cannot be undone; only setting and filter rule changes have an undo",
            row.id, row.action
        ));
    };
    if !UNDOABLE_STATES.contains(&row.state.as_str()) {
        return Some(format!(
            "action {} is {}; only a verified or ineffective {} can be undone",
            row.id,
            row.state,
            action.code()
        ));
    }
    if row.effect.as_ref().is_some_and(fleet::told) {
        return Some(format!(
            "action {} only told the fleet coordinator; there is nothing to undo",
            row.id
        ));
    }
    None
}

pub(crate) fn can_undo(row: &ActionRow) -> bool {
    why_not(row).is_none()
}

async fn revert(owners: &Owners, row: &ActionRow) -> Result<Value, Halt> {
    match Action::from_code(&row.action) {
        Some(Action::SetSetting) => setting::undo(owners, row).await,
        Some(Action::DisableFilterRule) => filter_rule::undo(owners, row).await,
        _ => Err(Halt::Refused(format!("action {} has no undo", row.id))),
    }
}

pub(crate) async fn undo(owners: &Owners, id: i64) -> Result<ActionRow, UndoError> {
    let web = owners.persist.web();
    let row = web
        .wright_action(id)
        .await?
        .ok_or(UndoError::NotFound(id))?;
    if let Some(reason) = why_not(&row) {
        return Err(UndoError::Conflict(reason));
    }
    match revert(owners, &row).await {
        Ok(undo) => {
            if !web.mark_wright_undone(id, &undo).await? {
                return Err(UndoError::Conflict(format!(
                    "action {id} changed state while it was being undone"
                )));
            }
        }
        Err(Halt::Refused(reason) | Halt::Failed(reason)) => {
            web.mark_wright_undo_conflict(id, &reason).await?;
            return Err(UndoError::Conflict(reason));
        }
        Err(Halt::Transient(reason)) => return Err(UndoError::Unavailable(reason)),
    }
    web.wright_action(id).await?.ok_or(UndoError::NotFound(id))
}
