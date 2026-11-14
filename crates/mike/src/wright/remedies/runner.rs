use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::Value;

use super::{
    Action, Halt, Owners, PAUSE_AFTER_REFUSALS, TURN_OFF_AFTER_INEFFECTIVE, dispatch, fleet,
};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::remedies::{Measured, PendingAction};

const BATCH: i64 = 50;
pub(crate) const RESUME_AFTER_SECS: f64 = 120.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Settled {
    Verified,
    Refused { paused: bool },
    Failed { paused: bool },
    Deferred(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PassReport {
    pub(crate) verified: usize,
    pub(crate) refused: usize,
    pub(crate) failed: usize,
    pub(crate) deferred: usize,
    pub(crate) paused: usize,
    pub(crate) measured: Vec<Measured>,
}

impl PassReport {
    fn note(&mut self, settled: &Settled) {
        match settled {
            Settled::Verified => self.verified += 1,
            Settled::Refused { paused } => {
                self.refused += 1;
                self.paused += usize::from(*paused);
            }
            Settled::Failed { paused } => {
                self.failed += 1;
                self.paused += usize::from(*paused);
            }
            Settled::Deferred(_) => self.deferred += 1,
        }
    }
}

fn action_of(item: &PendingAction) -> Result<Action, Halt> {
    Action::from_code(&item.action).ok_or_else(|| {
        Halt::Refused(format!(
            "remedy {} names action `{}`, which Wright does not know",
            item.remedy, item.action
        ))
    })
}

async fn known_prior(
    owners: &Owners,
    item: &PendingAction,
    action: Action,
    id: i64,
) -> Result<Value, Halt> {
    if let Some(prior) = &item.prior_state {
        return Ok(prior.clone());
    }
    let prior = dispatch::prior(owners, item, action).await?;
    owners.persist.web().record_wright_prior(id, &prior).await?;
    Ok(prior)
}

async fn act(
    owners: &Owners,
    item: &PendingAction,
    action: Action,
    id: i64,
) -> Result<Value, Halt> {
    let effect = match dispatch::seat(owners, item, action).await? {
        Some(seat) => match fleet::running(owners, seat).await? {
            Some(running) => fleet::tell(owners, item, id, &running, seat).await?,
            None => apply_owned(owners, item, action, id).await?,
        },
        None => apply_owned(owners, item, action, id).await?,
    };
    owners
        .persist
        .web()
        .mark_wright_applied(id, &effect)
        .await?;
    Ok(effect)
}

async fn apply_owned(
    owners: &Owners,
    item: &PendingAction,
    action: Action,
    id: i64,
) -> Result<Value, Halt> {
    let prior = known_prior(owners, item, action, id).await?;
    dispatch::apply(owners, item, action, id, &prior).await
}

async fn drive(owners: &Owners, item: &PendingAction, id: i64) -> Result<Value, Halt> {
    let action = action_of(item)?;
    let effect = match (item.state.as_deref(), &item.effect) {
        (Some("applied"), Some(effect)) => effect.clone(),
        _ => act(owners, item, action, id).await?,
    };
    if fleet::told(&effect) {
        return fleet::verify(owners, &effect).await;
    }
    dispatch::verify(owners, item, action, &effect).await
}

async fn settle(
    owners: &Owners,
    id: i64,
    result: Result<Value, Halt>,
) -> Result<Settled, PersistError> {
    let web = owners.persist.web();
    let (state, reason) = match result {
        Ok(effect) => {
            web.mark_wright_verified(id, &effect).await?;
            return Ok(Settled::Verified);
        }
        Err(Halt::Transient(reason)) => {
            tracing::warn!(action = id, %reason, "wright: an action is left to resume later");
            return Ok(Settled::Deferred(reason));
        }
        Err(Halt::Refused(reason)) => ("refused", reason),
        Err(Halt::Failed(reason)) => ("failed", reason),
    };
    let paused = web
        .refuse_wright_action(id, state, &reason, PAUSE_AFTER_REFUSALS)
        .await?
        .unwrap_or(false);
    Ok(match state {
        "refused" => Settled::Refused { paused },
        _ => Settled::Failed { paused },
    })
}

pub(crate) async fn run(
    owners: &Owners,
    item: &PendingAction,
) -> Result<Option<Settled>, PersistError> {
    let id = match item.action_id {
        Some(id) => id,
        None => {
            let claimed = owners
                .persist
                .web()
                .claim_wright_action(&item.remedy, item.problem_id, item.episode_id)
                .await?;
            let Some(id) = claimed else {
                return Ok(None);
            };
            id
        }
    };
    let result = drive(owners, item, id).await;
    settle(owners, id, result).await.map(Some)
}

pub(crate) async fn pass(owners: &Owners) -> Result<PassReport, PersistError> {
    let web = owners.persist.web();
    let mut items = web
        .unfinished_wright_actions(RESUME_AFTER_SECS, BATCH)
        .await?;
    items.extend(web.due_wright_actions(BATCH).await?);
    let mut report = PassReport::default();
    for item in &items {
        match run(owners, item).await {
            Ok(Some(settled)) => report.note(&settled),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, remedy = %item.remedy, problem = item.problem_id, "wright: a remedy could not be recorded")
            }
        }
    }
    report.measured = web
        .measure_wright_actions(TURN_OFF_AFTER_INEFFECTIVE)
        .await?;
    Ok(report)
}
