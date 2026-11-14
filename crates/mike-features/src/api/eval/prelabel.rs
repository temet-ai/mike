use std::collections::BTreeMap;
use std::sync::LazyLock;

use dashmap::{DashMap, DashSet};
use serde::Serialize;
use serde_json::{Value, json};

use crate::llm::decisions::env::{DecisionEnv, Pace, head_chars, tail_chars};
use crate::llm::decisions::point::{Asked, choice, record_outcome};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::eval_runs::turn_texts::PrelabelTurn;
use crate::store::persistence::operator_turns::projection::REACTIONS;

pub const POINT: &str = "label_prelabel";
pub const QUESTION_SET: &str = "label_prelabel_v1";
pub const SURE_ABOVE: f64 = 0.8;
const GROUP: usize = 5;
const MAX_CALLS: usize = 4;
const PROMPT_CHARS: usize = 600;
const ENDING_CHARS: usize = 900;
const NEXT_CHARS: usize = 600;

const MEANINGS: &[(&str, &str)] = &[
    (
        "corrected",
        "the operator's next message says the turn was wrong or asks to redo it",
    ),
    (
        "redirected",
        "the operator's next message moves on to different work without calling the turn wrong",
    ),
    (
        "accepted",
        "the operator's next message accepts the turn or builds on it",
    ),
    ("no_response", "the operator sent no further message"),
    (
        "unclear",
        "the next message does not show how the operator took the turn",
    ),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Prelabel {
    pub label: String,
    pub probability: f64,
    pub uncertain: bool,
    #[serde(skip)]
    pub record_id: Option<i64>,
}

static PRELABELS: LazyLock<DashMap<i64, Prelabel>> = LazyLock::new(DashMap::new);

fn question_key(turn_id: i64) -> String {
    format!("turn_{turn_id}")
}

pub fn questions(group: &[&PrelabelTurn]) -> BTreeMap<String, Question> {
    debug_assert_eq!(MEANINGS.len(), REACTIONS.len());
    group
        .iter()
        .map(|turn| {
            (
                question_key(turn.turn_id),
                choice(
                    &format!(
                        "How did the operator react to the assistant's work in turn {}?",
                        turn.turn_id
                    ),
                    MEANINGS,
                ),
            )
        })
        .collect()
}

pub fn state(group: &[&PrelabelTurn]) -> Value {
    json!({
        "turns": group
            .iter()
            .map(|t| json!({
                "turn_id": t.turn_id,
                "operator_prompt": head_chars(&t.user_prompt, PROMPT_CHARS),
                "assistant_ending": tail_chars(&t.final_text, ENDING_CHARS),
                "operator_next_message": if t.next_prompt.is_empty() { Value::Null } else { json!(head_chars(&t.next_prompt, NEXT_CHARS)) },
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn compose(
    asked: &Asked,
    group: &[&PrelabelTurn],
    record_id: Option<i64>,
) -> Vec<(i64, Prelabel)> {
    if asked.failed() {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|turn| {
            let (label, probability) = asked.choice(&question_key(turn.turn_id))?;
            REACTIONS.contains(&label).then(|| {
                (
                    turn.turn_id,
                    Prelabel {
                        label: label.to_owned(),
                        probability,
                        uncertain: probability < SURE_ABOVE,
                        record_id,
                    },
                )
            })
        })
        .collect()
}

static FILLING: LazyLock<DashSet<i64>> = LazyLock::new(DashSet::new);

pub fn cached(turn_ids: &[i64]) -> BTreeMap<i64, Prelabel> {
    turn_ids
        .iter()
        .filter_map(|id| PRELABELS.get(id).map(|p| (*id, p.clone())))
        .collect()
}

pub fn spawn_fill(env: Option<DecisionEnv>, persist: PersistHandle, turn_ids: &[i64]) {
    let Some(env) = env else {
        return;
    };
    let wanted: Vec<i64> = turn_ids
        .iter()
        .copied()
        .filter(|id| !PRELABELS.contains_key(id) && FILLING.insert(*id))
        .take(GROUP * MAX_CALLS)
        .collect();
    if wanted.is_empty() {
        return;
    }
    tokio::spawn(async move {
        fill(&env, &persist, &wanted).await;
        for id in &wanted {
            FILLING.remove(id);
        }
    });
}

async fn fill(env: &DecisionEnv, persist: &PersistHandle, missing: &[i64]) {
    let Some(decider) = env.decider(POINT) else {
        return;
    };
    let Ok(turns) = persist.eval().prelabel_turns(missing).await else {
        return;
    };
    let refs: Vec<&PrelabelTurn> = turns.iter().collect();
    let mut pace = Pace::new(MAX_CALLS);
    for group in refs.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let asked = decider
            .ask(
                QUESTION_SET,
                state(group),
                questions(group),
                DecisionSubject {
                    session_id: group.first().map(|t| t.session_id),
                    operator_turn_id: group.first().map(|t| t.turn_id),
                    ..DecisionSubject::default()
                },
                json!({ "sure_above": SURE_ABOVE }),
            )
            .await;
        pace.spent(&asked);
        let found = compose(&asked, group, asked.record_id);
        let score = if found.is_empty() {
            0.0
        } else {
            found.iter().map(|(_, p)| p.probability).sum::<f64>() / found.len() as f64
        };
        let applied = if found.is_empty() {
            "none"
        } else {
            "prelabelled"
        };
        decider
            .mark(&asked, asked.composed(json!({ "score": score })), applied)
            .await;
        for (id, prelabel) in found {
            PRELABELS.insert(id, prelabel);
        }
    }
}

pub fn attach(items: Vec<Value>, prelabels: &BTreeMap<i64, Prelabel>) -> Vec<Value> {
    items
        .into_iter()
        .map(|mut item| {
            let turn = item.get("operator_turn_id").and_then(Value::as_i64);
            if let (Some(prelabel), Some(map)) =
                (turn.and_then(|id| prelabels.get(&id)), item.as_object_mut())
                && let Ok(value) = serde_json::to_value(prelabel)
            {
                map.insert("prelabel".to_owned(), value);
            }
            item
        })
        .collect()
}

pub async fn record_human_label(persist: &PersistHandle, turn_id: i64, label: &str) {
    let Some((_, prelabel)) = PRELABELS.remove(&turn_id) else {
        return;
    };
    let Some(record_id) = prelabel.record_id else {
        return;
    };
    let verdict = if prelabel.label == label {
        "right"
    } else {
        "wrong"
    };
    record_outcome(
        persist,
        record_id,
        verdict,
        "operator_gold_label",
        json!({ "turn_id": turn_id, "prelabel": prelabel.label, "human": label }),
    )
    .await;
}

#[cfg(test)]
#[path = "prelabel_tests.rs"]
mod tests;
