use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::point::{Judged, Prepared, ReplayPoint};
use crate::analysis::judge::jev::questions::{
    ABANDONED, ACCEPTED, ASKS_OPERATOR, CORRECTED, DELIVERED, LEFT_OPEN, REDIRECTED, SHIPPED,
};
use crate::analysis::judge::jev::{
    self, Composed, POINT, QUESTION_SET, Shape, TurnFacts, UNCERTAIN_BELOW,
};
use crate::llm::decisions::point::Asked;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::decisions_replay::TurnJudgeItem;
use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::turn_verdicts::JudgeTurnContextRow;
use crate::store::persistence::web_repo::WebPersist as _;

const CONTEXT_BATCH: usize = 500;

pub(super) struct Ready {
    pub item: TurnJudgeItem,
    pub context: JudgeTurnContextRow,
}

pub(super) struct TurnJudge;

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let items = persist.web().replay_turn_judge_items(since, limit).await?;
    let mut contexts: HashMap<i64, JudgeTurnContextRow> = HashMap::new();
    for chunk in items.chunks(CONTEXT_BATCH) {
        let ids: Vec<i64> = chunk.iter().map(|i| i.turn_id).collect();
        for row in persist.eval().fetch_judge_turn_contexts(&ids).await? {
            contexts.insert(row.turn_id, row);
        }
    }
    let total = items.len();
    let ready: Vec<Ready> = items
        .into_iter()
        .filter_map(|item| {
            let context = contexts.remove(&item.turn_id)?;
            Some(Ready { item, context })
        })
        .collect();
    let skipped = total - ready.len();
    Ok((ready, skipped))
}

fn facts(context: &JudgeTurnContextRow) -> TurnFacts<'_> {
    TurnFacts {
        turn_id: context.turn_id,
        session_id: Some(context.session_id),
        operator_request: &context.user_prompt,
        assistant_text: &context.final_assistant_text,
        tool_call_count: context.tool_call_count,
        tool_error_count: context.tool_error_count,
        next_message: context.next_genuine_text.as_deref(),
    }
}

fn shape(context: &JudgeTurnContextRow) -> Shape {
    Shape::TwoAxis {
        has_next: context.next_genuine_text.is_some(),
    }
}

fn axis_positive(question: &str, reaction: Option<&str>, delivery: Option<&str>) -> Option<bool> {
    match question {
        CORRECTED => reaction.map(|r| r == "corrected"),
        REDIRECTED => reaction.map(|r| r == "redirected"),
        ACCEPTED => reaction.map(|r| r == "accepted"),
        ABANDONED => delivery.map(|d| d == "none"),
        SHIPPED => delivery.map(|d| d == "shipped"),
        DELIVERED => delivery.map(|d| d == "shipped" || d == "delivered"),
        LEFT_OPEN => delivery.map(|d| d == "partial"),
        ASKS_OPERATOR => delivery.map(|d| d == "question"),
        _ => None,
    }
}

fn agrees_with_current(item: &TurnJudgeItem, composed: &Composed) -> Option<bool> {
    match (
        item.live_reaction.as_deref(),
        item.live_delivery.as_deref(),
        item.live_legacy_label.as_deref(),
    ) {
        (Some(reaction), Some(delivery), _) => {
            Some(composed.reaction.code() == reaction && composed.delivery.code() == delivery)
        }
        (_, _, Some(label)) => Some(composed.label.outcome_code() == label),
        _ => None,
    }
}

impl ReplayPoint for TurnJudge {
    type Item = Ready;
    const POINT: &'static str = POINT;
    const QUESTION_SET: &'static str = QUESTION_SET;
    const OUTCOME_SOURCE: &'static str = "operator_label";

    fn item_id(item: &Ready) -> String {
        item.item.turn_id.to_string()
    }

    fn item_at(item: &Ready) -> DateTime<Utc> {
        item.item.closed_at
    }

    fn label(item: &Ready) -> String {
        item.item.outcome.clone()
    }

    fn prepare(item: &Ready) -> Prepared {
        let shape = shape(&item.context);
        let with_next = matches!(shape, Shape::TwoAxis { has_next: true });
        Prepared {
            state: jev::state::state(&facts(&item.context), with_next),
            questions: jev::questions::questions(shape),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let composed = jev::compose(asked, shape(&item.context))?;
        let right = composed.label.outcome_code() == item.item.outcome;
        Some(Judged {
            composed: composed.json(UNCERTAIN_BELOW as f32),
            score: composed.confidence,
            positive: Some(right),
            decision_right: Some(right),
            agrees_current: agrees_with_current(&item.item, &composed),
        })
    }

    fn question_positive(item: &Ready, question: &str) -> Option<bool> {
        axis_positive(
            question,
            item.item.gold_reaction.as_deref(),
            item.item.gold_delivery.as_deref(),
        )
    }

    fn current_positive(item: &Ready, question: &str) -> Option<bool> {
        axis_positive(
            question,
            item.item.live_reaction.as_deref(),
            item.item.live_delivery.as_deref(),
        )
    }
}
