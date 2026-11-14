use anyhow::Result;
use chrono::{DateTime, Utc};

use super::point::{Judged, Prepared, ReplayPoint};
use crate::llm::decisions::point::Asked;
use crate::steerer::correction_classifier::questions::CORRECTS as CORRECTS_QUESTION;
use crate::steerer::correction_classifier::{self, POINT, QUESTION_SET};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::decisions_replay::CorrectionItem;
use crate::store::persistence::web_repo::WebPersist as _;

pub(super) struct Ready {
    pub item: CorrectionItem,
    pub prior_turn: String,
}

pub(super) struct Correction;

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let items = persist.web().replay_correction_items(since, limit).await?;
    let total = items.len();
    let mut ready = Vec::with_capacity(total);
    for item in items {
        if let Some(prior_turn) =
            correction_classifier::prior_turn_end(persist, item.session_id, item.request_id).await
        {
            ready.push(Ready { item, prior_turn });
        }
    }
    let skipped = total - ready.len();
    Ok((ready, skipped))
}

fn keyword_says_correction(item: &CorrectionItem) -> Option<bool> {
    item.keyword_kind
        .as_deref()
        .filter(|kind| *kind != "initial")
        .map(|kind| kind == "correction")
}

impl ReplayPoint for Correction {
    type Item = Ready;
    const POINT: &'static str = POINT;
    const QUESTION_SET: &'static str = QUESTION_SET;
    const OUTCOME_SOURCE: &'static str = "operator_kind";

    fn item_id(item: &Ready) -> String {
        item.item.entry_id.to_string()
    }

    fn item_at(item: &Ready) -> DateTime<Utc> {
        item.item.created_at
    }

    fn label(item: &Ready) -> String {
        item.item.kind.clone()
    }

    fn prepare(item: &Ready) -> Prepared {
        Prepared {
            state: correction_classifier::state::state(&item.item.content, &item.prior_turn),
            questions: correction_classifier::questions::questions(),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let classified = correction_classifier::compose(asked)?;
        let current = keyword_says_correction(&item.item);
        Some(Judged {
            composed: classified.json(),
            score: classified.probability,
            positive: Some(item.item.kind == "correction"),
            decision_right: Some(classified.kind == item.item.kind),
            agrees_current: current.map(|c| c == (classified.kind == "correction")),
        })
    }

    fn question_positive(item: &Ready, question: &str) -> Option<bool> {
        (question == CORRECTS_QUESTION).then(|| item.item.kind == "correction")
    }

    fn current_positive(item: &Ready, question: &str) -> Option<bool> {
        if question == CORRECTS_QUESTION {
            keyword_says_correction(&item.item)
        } else {
            None
        }
    }
}
