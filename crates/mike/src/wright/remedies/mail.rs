use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Halt, Owners};
use crate::store::persistence::wright::remedies::PendingAction;

fn recipient(item: &PendingAction) -> Result<Uuid, Halt> {
    item.resource.trim().parse().map_err(|_| {
        Halt::Refused(format!(
            "problem {} names resource `{}`, which is not a session uuid",
            item.problem_id, item.resource
        ))
    })
}

pub(crate) fn seat(item: &PendingAction) -> Result<Option<Uuid>, Halt> {
    recipient(item).map(Some)
}

fn messages(prior: &Value) -> Result<Vec<Uuid>, Halt> {
    serde_json::from_value(prior["messages"].clone()).map_err(|e| {
        Halt::Failed(format!(
            "the stored prior state has no message list ({e}): {prior}"
        ))
    })
}

pub(crate) async fn prior(owners: &Owners, item: &PendingAction) -> Result<Value, Halt> {
    let to = recipient(item)?;
    let ceiling = i64::from(item.max_resources);
    let waiting = owners
        .persist
        .web()
        .wright_waiting_mail(to, ceiling + 1)
        .await?;
    if i64::try_from(waiting.len()).unwrap_or(i64::MAX) > ceiling {
        return Err(Halt::Refused(format!(
            "more than {ceiling} messages to session {to} wait for a sender notice; the remedy's ceiling is {ceiling}"
        )));
    }
    if waiting.is_empty() {
        return Err(Halt::Refused(format!(
            "no undelivered message to session {to} has a sender left to tell"
        )));
    }
    Ok(json!({"recipient": to, "messages": waiting, "sender_notified": false}))
}

pub(crate) async fn apply(owners: &Owners, prior: &Value) -> Result<Value, Halt> {
    let messages = messages(prior)?;
    let notices = owners
        .persist
        .launch()
        .tell_senders_not_delivered(&messages)
        .await?;
    Ok(json!({
        "outcome": "told_senders",
        "recipient": prior["recipient"],
        "messages": messages,
        "notices": notices,
    }))
}

pub(crate) async fn verify(
    owners: &Owners,
    item: &PendingAction,
    effect: &Value,
) -> Result<Value, Halt> {
    let to = recipient(item)?;
    let messages = messages(effect)?;
    let web = owners.persist.web();
    let ceiling = i64::try_from(messages.len()).unwrap_or(i64::MAX) + i64::from(item.max_resources);
    let still_waiting: Vec<Uuid> = web
        .wright_waiting_mail(to, ceiling)
        .await?
        .into_iter()
        .filter(|uuid| messages.contains(uuid))
        .collect();
    if !still_waiting.is_empty() {
        return Err(Halt::Failed(format!(
            "{} of {} messages to session {to} still wait for a sender notice when read again",
            still_waiting.len(),
            messages.len()
        )));
    }
    let stamped = web.wright_mail_notified(&messages).await?;
    Ok(json!({
        "outcome": "told_senders",
        "recipient": to,
        "messages": messages,
        "notices": effect["notices"],
        "stamped": stamped.len(),
    }))
}
