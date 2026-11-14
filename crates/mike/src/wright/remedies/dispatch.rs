use serde_json::Value;
use uuid::Uuid;

use super::{Action, Halt, Owners, filter_rule, launch, mail, setting};
use crate::store::persistence::wright::remedies::PendingAction;

pub(crate) async fn seat(
    owners: &Owners,
    item: &PendingAction,
    action: Action,
) -> Result<Option<Uuid>, Halt> {
    match action {
        Action::CloseOrphanLaunch => launch::seat(owners, item).await,
        Action::NotifyUndeliveredSender => mail::seat(item),
        Action::SetSetting | Action::DisableFilterRule => Ok(None),
    }
}

pub(crate) async fn prior(
    owners: &Owners,
    item: &PendingAction,
    action: Action,
) -> Result<Value, Halt> {
    match action {
        Action::CloseOrphanLaunch => launch::prior(owners, item).await,
        Action::NotifyUndeliveredSender => mail::prior(owners, item).await,
        Action::SetSetting => setting::prior(owners, item).await,
        Action::DisableFilterRule => filter_rule::prior(owners, item).await,
    }
}

pub(crate) async fn apply(
    owners: &Owners,
    item: &PendingAction,
    action: Action,
    action_id: i64,
    prior: &Value,
) -> Result<Value, Halt> {
    match action {
        Action::CloseOrphanLaunch => launch::apply(owners, item, action_id).await,
        Action::NotifyUndeliveredSender => mail::apply(owners, prior).await,
        Action::SetSetting => setting::apply(owners, item, action_id).await,
        Action::DisableFilterRule => filter_rule::apply(owners, item).await,
    }
}

pub(crate) async fn verify(
    owners: &Owners,
    item: &PendingAction,
    action: Action,
    effect: &Value,
) -> Result<Value, Halt> {
    match action {
        Action::CloseOrphanLaunch => launch::verify(owners, item).await,
        Action::NotifyUndeliveredSender => mail::verify(owners, item, effect).await,
        Action::SetSetting => setting::verify(owners, effect).await,
        Action::DisableFilterRule => filter_rule::verify(owners, item).await,
    }
}
