use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};

use super::tagger::AnchorHit;
use crate::llm::decisions::env::{DecisionEnv, Pace, head_chars, tail_chars};
use crate::llm::decisions::point::{Asked, choice};
use crate::llm::decisions::{Question, record::DecisionSubject};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::semantic::{CorrectionForTagging, NewAssignment};

pub const POINT: &str = "semantic_tag";
pub const QUESTION_SET: &str = "semantic_tag_v1";
pub const NONE_KEY: &str = "none";
pub const MIN_PATTERN: f64 = 0.5;
const GROUP: usize = 6;
const MAX_CALLS: usize = 8;
const TEXT_CHARS: usize = 500;

pub struct Unit<'a> {
    pub correction: &'a CorrectionForTagging,
    pub text: String,
    pub anchors: Vec<(AnchorHit, String)>,
}

fn question_key(item_id: i64) -> String {
    format!("correction_{item_id}")
}

fn patterns(unit: &Unit<'_>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for (anchor, _) in &unit.anchors {
        if !anchor.pattern.is_empty() && !seen.contains(&anchor.pattern) {
            seen.push(anchor.pattern.clone());
        }
    }
    seen
}

pub fn questions(group: &[Unit<'_>]) -> BTreeMap<String, Question> {
    group
        .iter()
        .map(|unit| {
            let mut options: Vec<(String, String)> = patterns(unit)
                .into_iter()
                .map(|p| {
                    let meaning = format!("the same kind of correction as the {p} examples");
                    (p, meaning)
                })
                .collect();
            options.push((
                NONE_KEY.to_owned(),
                "none of these patterns matches the correction".to_owned(),
            ));
            let refs: Vec<(&str, &str)> = options
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            (
                question_key(unit.correction.item_id),
                choice(
                    &format!(
                        "Which correction pattern does correction {} match? Choose a pattern only when the operator is correcting the same kind of mistake as its examples.",
                        unit.correction.item_id
                    ),
                    &refs,
                ),
            )
        })
        .collect()
}

pub fn state(group: &[Unit<'_>]) -> Value {
    json!({
        "corrections": group
            .iter()
            .map(|unit| json!({
                "correction_item_id": unit.correction.item_id,
                "text": head_chars(&unit.text, TEXT_CHARS),
                "pattern_examples": unit.anchors.iter().map(|(a, text)| json!({
                    "pattern": a.pattern,
                    "similarity": (a.similarity * 1000.0).round() / 1000.0,
                    "text": tail_chars(text, TEXT_CHARS),
                })).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn compose(asked: &Asked, group: &[Unit<'_>]) -> Vec<(i64, Option<String>, f64)> {
    if asked.failed() {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|unit| {
            let (key, p) = asked.choice(&question_key(unit.correction.item_id))?;
            let id = unit.correction.item_id;
            if key == NONE_KEY || p < MIN_PATTERN {
                return Some((id, None, p));
            }
            patterns(unit)
                .iter()
                .any(|known| known == key)
                .then(|| (id, Some(key.to_owned()), p))
        })
        .collect()
}

fn apply(rows: &mut [NewAssignment], group: &[Unit<'_>], decided: &[(i64, Option<String>, f64)]) {
    for (id, pattern, _) in decided {
        let Some(row) = rows.iter_mut().find(|r| r.correction_item_id == *id) else {
            continue;
        };
        let anchor = group
            .iter()
            .find(|u| u.correction.item_id == *id)
            .and_then(|u| {
                u.anchors
                    .iter()
                    .find(|(a, _)| Some(&a.pattern) == pattern.as_ref())
                    .map(|(a, _)| a.item_id)
            });
        row.pattern.clone_from(pattern);
        if anchor.is_some() {
            row.anchor_item_id = anchor;
        }
    }
}

fn mean(decided: &[(i64, Option<String>, f64)]) -> f64 {
    if decided.is_empty() {
        return 0.0;
    }
    decided.iter().map(|(_, _, p)| p).sum::<f64>() / decided.len() as f64
}

async fn texts(
    persist: &PersistHandle,
    corrections: &[&CorrectionForTagging],
    tops: &[&Vec<AnchorHit>],
) -> HashMap<i64, String> {
    let mut ids: Vec<i64> = corrections.iter().map(|c| c.item_id).collect();
    ids.extend(tops.iter().flat_map(|t| t.iter().map(|a| a.item_id)));
    ids.sort_unstable();
    ids.dedup();
    persist
        .knowledge()
        .semantic_item_texts(&ids)
        .await
        .map(|rows| rows.into_iter().collect())
        .unwrap_or_default()
}

pub async fn refine(
    env: Option<&DecisionEnv>,
    persist: &PersistHandle,
    corrections: &[CorrectionForTagging],
    rows: &mut [NewAssignment],
    tops: &[Vec<AnchorHit>],
) {
    let Some(decider) = env.and_then(|e| e.decider(POINT)) else {
        return;
    };
    let wanted: Vec<usize> = (0..corrections.len())
        .filter(|&i| {
            tops.get(i)
                .is_some_and(|t| t.iter().any(|a| !a.pattern.is_empty()))
        })
        .take(GROUP * MAX_CALLS)
        .collect();
    let picked_c: Vec<&CorrectionForTagging> = wanted.iter().map(|&i| &corrections[i]).collect();
    let picked_t: Vec<&Vec<AnchorHit>> = wanted.iter().map(|&i| &tops[i]).collect();
    let by_id = texts(persist, &picked_c, &picked_t).await;
    let units: Vec<Unit<'_>> = picked_c
        .iter()
        .zip(&picked_t)
        .map(|(c, t)| Unit {
            correction: c,
            text: by_id.get(&c.item_id).cloned().unwrap_or_default(),
            anchors: t
                .iter()
                .map(|a| {
                    (
                        a.clone(),
                        by_id.get(&a.item_id).cloned().unwrap_or_default(),
                    )
                })
                .collect(),
        })
        .collect();
    let mut pace = Pace::new(MAX_CALLS);
    for group in units.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let asked = decider
            .ask(
                QUESTION_SET,
                state(group),
                questions(group),
                DecisionSubject::default(),
                json!({ "min_pattern": MIN_PATTERN }),
            )
            .await;
        pace.spent(&asked);
        let decided = compose(&asked, group);
        let applied = if decided.iter().any(|(_, p, _)| p.is_some()) {
            "chosen"
        } else {
            "none"
        };
        decider
            .mark(
                &asked,
                asked.composed(json!({ "score": mean(&decided) })),
                applied,
            )
            .await;
        if decider.applies() {
            apply(rows, group, &decided);
        }
    }
}

#[cfg(test)]
#[path = "tag_decide_tests.rs"]
mod tests;
