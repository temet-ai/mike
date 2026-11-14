use anyhow::Result;
use chrono::{DateTime, Utc};

use super::point::{Judged, Prepared, ReplayPoint};
use crate::dossier::steering_facts::SteeringFacts;
use crate::llm::decisions::point::Asked;
use crate::quiescence_worker::wake_recheck::questions::{DONE_SINCE, STILL_OPEN};
use crate::quiescence_worker::wake_recheck::state::WakeInput;
use crate::quiescence_worker::wake_recheck::{self, POINT, QUESTION_SET};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::decisions_replay::WakeItem;
use crate::store::persistence::web_repo::WebPersist as _;

pub(super) struct Ready {
    pub item: WakeItem,
    pub facts: Option<SteeringFacts>,
}

pub(super) struct WakeRecheck;

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let items = persist.web().replay_wake_items(since, limit).await?;
    let mut ready = Vec::with_capacity(items.len());
    for item in items {
        let facts = SteeringFacts::as_of(
            persist,
            item.session_id,
            item.operator_turn_id,
            item.decided_at,
        )
        .await
        .ok();
        ready.push(Ready { item, facts });
    }
    Ok((ready, 0))
}

fn fire_right(item: &WakeItem) -> bool {
    item.label == "right"
}

impl ReplayPoint for WakeRecheck {
    type Item = Ready;
    const POINT: &'static str = POINT;
    const QUESTION_SET: &'static str = QUESTION_SET;
    const OUTCOME_SOURCE: &'static str = "turn_guard_label";

    fn item_id(item: &Ready) -> String {
        item.item.intervention_uuid.to_string()
    }

    fn item_at(item: &Ready) -> DateTime<Utc> {
        item.item.decided_at
    }

    fn label(item: &Ready) -> String {
        item.item.label.clone()
    }

    fn prepare(item: &Ready) -> Prepared {
        let input = WakeInput {
            nudge: &item.item.nudge,
            question: item.item.question.as_deref(),
            facts: item.facts.as_ref(),
        };
        Prepared {
            state: wake_recheck::state::state(&input),
            questions: wake_recheck::questions::questions(),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let recheck = wake_recheck::compose(asked)?;
        let right = fire_right(&item.item);
        Some(Judged {
            composed: recheck.json(!recheck.skip),
            score: recheck.still_needed,
            positive: Some(right),
            decision_right: Some(recheck.skip != right),
            agrees_current: None,
        })
    }

    fn question_positive(item: &Ready, question: &str) -> Option<bool> {
        let right = fire_right(&item.item);
        match question {
            STILL_OPEN => Some(right),
            DONE_SINCE => Some(!right),
            _ => None,
        }
    }

    fn current_positive(_item: &Ready, _question: &str) -> Option<bool> {
        None
    }
}
