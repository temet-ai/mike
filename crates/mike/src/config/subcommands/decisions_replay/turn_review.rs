use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use super::turn_judge::{facts, shape};
use crate::analysis::judge::jev::{self, Composed, POINT, QUESTION_SET, Shape};
use crate::llm::decisions::point::Asked;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::turn_verdicts::JudgeTurnContextRow;

const AXES: [&str; 3] = ["reaction", "delivery", "label"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Gold {
    pub reaction: Option<String>,
    pub delivery: Option<String>,
    pub label: Option<String>,
}

pub(super) struct Ready {
    pub record: Labelled,
    pub context: JudgeTurnContextRow,
    pub gold: Gold,
}

pub(super) struct TurnReview;

fn axis<'a>(value: Option<&'a Value>, name: &str) -> Option<&'a str> {
    value?.get(name)?.as_str()
}

fn gold(record: &Labelled) -> Option<Gold> {
    let verdict = record.composed.as_ref()?.get("verdict");
    if record.right() {
        return Some(Gold {
            reaction: axis(verdict, "reaction").map(str::to_owned),
            delivery: axis(verdict, "delivery").map(str::to_owned),
            label: None,
        });
    }
    let expected = record.evidence.get("expected")?;
    let named = |name: &str| axis(Some(expected), name).map(str::to_owned);
    if AXES.iter().all(|a| expected.get(*a).is_none()) {
        return None;
    }
    let label = named("label");
    let keep_old = |name: &str| {
        named(name).or_else(|| {
            label
                .is_none()
                .then(|| axis(verdict, name).map(str::to_owned))
                .flatten()
        })
    };
    Some(Gold {
        reaction: keep_old("reaction"),
        delivery: keep_old("delivery"),
        label: label.clone(),
    })
}

fn matches(gold: &Gold, composed: &Composed) -> bool {
    gold.reaction
        .as_deref()
        .is_none_or(|g| g == composed.reaction.code())
        && gold
            .delivery
            .as_deref()
            .is_none_or(|g| g == composed.delivery.code())
        && gold
            .label
            .as_deref()
            .is_none_or(|g| g == composed.label.outcome_code())
}

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let records = labelled::load(persist, POINT, since, limit).await?;
    let total = records.len();
    let ids: Vec<i64> = records.iter().filter_map(|r| r.operator_turn_id).collect();
    let contexts: HashMap<i64, JudgeTurnContextRow> = persist
        .eval()
        .fetch_judge_turn_contexts(&ids)
        .await?
        .into_iter()
        .map(|row| (row.turn_id, row))
        .collect();
    let out: Vec<Ready> = records
        .into_iter()
        .filter_map(|record| {
            let gold = gold(&record)?;
            let context = contexts.get(&record.operator_turn_id?)?.clone();
            Some(Ready {
                record,
                context,
                gold,
            })
        })
        .collect();
    let skipped = total - out.len();
    Ok((out, skipped))
}

impl ReplayPoint for TurnReview {
    type Item = Ready;
    const POINT: &'static str = POINT;
    const QUESTION_SET: &'static str = QUESTION_SET;
    const OUTCOME_SOURCE: &'static str = labelled::SOURCE;

    fn item_id(item: &Ready) -> String {
        item.record.record_id.to_string()
    }

    fn item_at(item: &Ready) -> DateTime<Utc> {
        item.record.created_at
    }

    fn label(item: &Ready) -> String {
        format!("{} expected={:?}", item.record.label, item.gold)
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
        let right = matches(&item.gold, &composed);
        Some(Judged {
            composed: json!({
                "reaction": composed.reaction.code(),
                "delivery": composed.delivery.code(),
                "label": composed.label.outcome_code(),
                "expected": format!("{:?}", item.gold),
            }),
            score: composed.confidence,
            positive: Some(right),
            decision_right: Some(right),
            agrees_current: Some(item.record.right() == right),
        })
    }

    fn question_positive(_: &Ready, _: &str) -> Option<bool> {
        None
    }

    fn current_positive(_: &Ready, _: &str) -> Option<bool> {
        None
    }
}
