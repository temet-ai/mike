use sqlx::PgPool;
use uuid::Uuid;

use super::envelope::envelope;
use super::store::{self, Mail};
use super::{Delivery, Found, Mode, Reach, Recipient, Sent, Sighting, Stage};

const ROUNDS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Missing,
    AlreadySettled(String),
    Delivered(Sighting),
    Accepted { turn_id: String },
    Waiting(String),
    Unresolved(String),
}

enum Next {
    Done(Outcome),
    Retry,
}

pub async fn deliver(
    pool: &PgPool,
    recipient: &dyn Recipient,
    message: Uuid,
) -> Result<Outcome, sqlx::Error> {
    for _ in 0..ROUNDS {
        if let Next::Done(outcome) = round(pool, recipient, message).await? {
            return Ok(outcome);
        }
    }
    Ok(Outcome::Waiting(format!(
        "{ROUNDS} delivery rounds ended without an accepted attempt"
    )))
}

async fn round(
    pool: &PgPool,
    recipient: &dyn Recipient,
    message: Uuid,
) -> Result<Next, sqlx::Error> {
    let Some(mail) = store::load(pool, message).await? else {
        return Ok(Next::Done(Outcome::Missing));
    };
    if matches!(mail.status.as_str(), "delivered" | "failed") {
        return Ok(Next::Done(Outcome::AlreadySettled(mail.status)));
    }
    let Some(generation) = recipient.generation().await else {
        return Ok(Next::Done(Outcome::Waiting(
            "the recipient has no live owner to deliver to".into(),
        )));
    };
    if generation != mail.generation {
        store::rebind(pool, message, generation).await?;
    }
    if let Some(done) = reconcile(pool, recipient, &mail).await? {
        return Ok(Next::Done(done));
    }
    if let Some(latest) = &mail.latest
        && latest.outcome.as_deref() == Some("accepted")
        && latest.generation == generation
    {
        return Ok(Next::Done(Outcome::Waiting(format!(
            "attempt {} was accepted into turn {}; no consumption seen yet",
            latest.number,
            latest.turn_id.as_deref().unwrap_or("unknown"),
        ))));
    }
    let mode = match recipient.reach().await {
        Reach::Idle => Mode::Start,
        Reach::Busy { turn_id } => Mode::Steer {
            expected_turn: turn_id,
        },
        Reach::Unreachable(reason) => return Ok(Next::Done(Outcome::Waiting(reason))),
    };
    attempt(pool, recipient, &mail, generation, mode).await
}

async fn reconcile(
    pool: &PgPool,
    recipient: &dyn Recipient,
    mail: &Mail,
) -> Result<Option<Outcome>, sqlx::Error> {
    let maybe_sent = mail.latest.as_ref().is_some_and(|a| a.maybe_delivered());
    match recipient.find(mail.message).await {
        Found::Seen(sighting) => record_sighting(pool, recipient, mail.message, sighting)
            .await
            .map(Some),
        Found::Unknown(reason) if maybe_sent => {
            let reason = format!(
                "an earlier attempt may have reached the recipient and its state cannot be read: {reason}"
            );
            store::settle(pool, mail.message, "unresolved", Some(&reason)).await?;
            Ok(Some(Outcome::Unresolved(reason)))
        }
        Found::Unknown(_) | Found::NotSeen => Ok(None),
    }
}

async fn record_sighting(
    pool: &PgPool,
    recipient: &dyn Recipient,
    message: Uuid,
    sighting: Sighting,
) -> Result<Outcome, sqlx::Error> {
    for (stage, evidence) in &sighting.stages {
        store::record_stage(pool, message, *stage, None, evidence).await?;
    }
    if !sighting.has(Stage::Consumed) {
        let turn_id = sighting.turn_id.clone().unwrap_or_default();
        return Ok(Outcome::Accepted { turn_id });
    }
    recipient.taken(message, true).await;
    store::settle(pool, message, "delivered", None).await?;
    Ok(Outcome::Delivered(sighting))
}

async fn attempt(
    pool: &PgPool,
    recipient: &dyn Recipient,
    mail: &Mail,
    generation: i64,
    mode: Mode,
) -> Result<Next, sqlx::Error> {
    let attempt = store::begin_attempt(pool, mail.message, generation, &mode).await?;
    let text = envelope(mail.message, mail.from, mail.scope.as_deref(), &mail.body);
    let sent = recipient
        .send(&Delivery {
            message: mail.message,
            attempt,
            text: &text,
            mode,
        })
        .await;
    let (outcome, turn, detail) = match &sent {
        Sent::Accepted { turn_id } => ("accepted", Some(turn_id.as_str()), None),
        Sent::TurnEnded(d) => ("turn_ended", None, Some(d.as_str())),
        Sent::Refused(d) => ("refused", None, Some(d.as_str())),
        Sent::FailedBeforeSend(d) => ("failed_before_send", None, Some(d.as_str())),
        Sent::FailedAfterSend(d) => ("failed_after_send", None, Some(d.as_str())),
    };
    store::finish_attempt(pool, attempt, outcome, turn, detail).await?;
    match sent {
        Sent::Accepted { turn_id } => {
            let evidence = format!("the runtime accepted attempt {attempt} into turn {turn_id}");
            store::record_stage(
                pool,
                mail.message,
                Stage::Accepted,
                Some(attempt),
                &evidence,
            )
            .await?;
            recipient.taken(mail.message, false).await;
            Ok(Next::Done(Outcome::Accepted { turn_id }))
        }
        Sent::TurnEnded(_) | Sent::FailedAfterSend(_) => Ok(Next::Retry),
        Sent::Refused(d) | Sent::FailedBeforeSend(d) => Ok(Next::Done(Outcome::Waiting(d))),
    }
}
