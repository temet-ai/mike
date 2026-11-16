use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use crate::dossier::directive_point::compose::{DECIDE_AT, compose};
use crate::dossier::directive_point::questions::{POINT, QUESTION_SET, key, questions};
use crate::dossier::directive_point::state::{Unit, sentence_around, state};
use crate::dossier::entries::directives::extract;
use crate::llm::decisions::point::Asked;
use crate::store::persistence::PersistHandle;

pub(super) struct Ready {
    pub record: Labelled,
    pub units: Vec<Unit>,
    pub expected: Vec<String>,
}

pub(super) struct Directive;

fn rebuilt(stored: &Value) -> Option<Unit> {
    let turn = stored.get("turn")?.as_str()?;
    let class = stored.get("action_class")?.as_str()?;
    let found = extract(turn).into_iter().find(|f| f.class == class)?;
    Some(Unit {
        turn_id: 0,
        session_id: 0,
        request_id: 0,
        class: found.class,
        sentence: sentence_around(turn, found.span),
        turn_text: turn.to_owned(),
    })
}

fn ready(record: Labelled) -> Option<Ready> {
    let stored = record.state.get("units")?.as_object()?;
    let mut units = Vec::new();
    let mut expected = Vec::new();
    for index in 0..stored.len() {
        let old_key = key(index);
        let unit = rebuilt(stored.get(&old_key)?)?;
        let want = record
            .expected_for(&old_key)
            .and_then(Value::as_str)
            .or_else(|| record.old_choice(&old_key))?;
        units.push(unit);
        expected.push(want.to_owned());
    }
    Some(Ready {
        record,
        units,
        expected,
    })
}

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let records = labelled::load(persist, POINT, since, limit).await?;
    let total = records.len();
    let out: Vec<Ready> = records.into_iter().filter_map(ready).collect();
    let skipped = total - out.len();
    Ok((out, skipped))
}

fn group(item: &Ready) -> Vec<&Unit> {
    item.units.iter().collect()
}

impl ReplayPoint for Directive {
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
        format!("{} expected={}", item.record.label, item.expected.join(","))
    }

    fn prepare(item: &Ready) -> Prepared {
        let group = group(item);
        Prepared {
            state: state(&group),
            questions: questions(&group),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let group = group(item);
        let decided = compose(asked, &group);
        let chosen: Vec<Option<&str>> = (0..group.len())
            .map(|index| {
                asked
                    .choice(&key(index))
                    .filter(|(_, p)| *p >= DECIDE_AT)
                    .map(|(c, _)| c)
            })
            .collect();
        let right = chosen.len() == item.expected.len()
            && chosen
                .iter()
                .zip(&item.expected)
                .all(|(c, want)| *c == Some(want.as_str()));
        let lowest = decided
            .iter()
            .map(|d| d.probability)
            .fold(1.0_f64, f64::min);
        Some(Judged {
            composed: json!({ "chosen": chosen, "expected": item.expected }),
            score: lowest,
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
