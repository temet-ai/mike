use anyhow::Result;
use chrono::{DateTime, Utc};

use super::point::{Judged, Prepared, ReplayPoint};
use crate::llm::decisions::point::Asked;
use crate::steerer::verdict::{Need, SteererVerdict};
use crate::steerer::verifier_precheck::questions::{DISCLOSED, ONLY_ANSWER, UNFINISHED};
use crate::steerer::verifier_precheck::{self, FireInput, POINT, QUESTION_SET};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::decisions_replay::VerifierItem;
use crate::store::persistence::web_repo::WebPersist as _;

pub(super) struct Ready {
    pub item: VerifierItem,
    pub verdict: SteererVerdict,
}

pub(super) struct VerifierPrecheck;

pub(super) fn verdict_of(item: &VerifierItem) -> SteererVerdict {
    item.verdict_json
        .clone()
        .and_then(|json| serde_json::from_value(json).ok())
        .unwrap_or_else(|| SteererVerdict {
            fires: true,
            confidence: 0.0,
            reason: String::new(),
            nudge: item.nudge.clone(),
            need: Need::default(),
            checklist: Vec::new(),
        })
}

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let items = persist.web().replay_verifier_items(since, limit).await?;
    let ready = items
        .into_iter()
        .map(|item| {
            let verdict = verdict_of(&item);
            Ready { item, verdict }
        })
        .collect();
    Ok((ready, 0))
}

fn fire_right(item: &VerifierItem) -> bool {
    item.label == "right"
}

impl ReplayPoint for VerifierPrecheck {
    type Item = Ready;
    const POINT: &'static str = POINT;
    const QUESTION_SET: &'static str = QUESTION_SET;
    const OUTCOME_SOURCE: &'static str = "turn_guard_label";

    fn item_id(item: &Ready) -> String {
        item.item.intervention_uuid.to_string()
    }

    fn item_at(item: &Ready) -> DateTime<Utc> {
        item.item.dispatched_at
    }

    fn label(item: &Ready) -> String {
        item.item.label.clone()
    }

    fn prepare(item: &Ready) -> Prepared {
        let input = FireInput {
            operator_ask: &item.item.operator_ask,
            turn_end: &item.item.turn_end,
            nudge: &item.item.nudge,
            verdict: &item.verdict,
        };
        Prepared {
            state: verifier_precheck::state::state(&input),
            questions: verifier_precheck::questions::questions(),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let verdict = verifier_precheck::compose(asked)?;
        let right = fire_right(&item.item);
        Some(Judged {
            composed: verdict.json(),
            score: verdict.score,
            positive: Some(right),
            decision_right: Some(verdict.send == right),
            agrees_current: None,
        })
    }

    fn question_positive(item: &Ready, question: &str) -> Option<bool> {
        let right = fire_right(&item.item);
        match question {
            UNFINISHED => Some(right),
            DISCLOSED | ONLY_ANSWER => Some(!right),
            _ => None,
        }
    }

    fn current_positive(_item: &Ready, _question: &str) -> Option<bool> {
        None
    }
}
