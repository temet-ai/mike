use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use crate::llm::decisions::point::Asked;
use crate::steerer::checklist_point::questions::{key, questions};
use crate::steerer::checklist_point::state::state;
use crate::steerer::checklist_point::{Criterion, Evidence, POINT, QUESTION_SET, compose};
use crate::store::persistence::PersistHandle;

pub(super) struct Ready {
    pub record: Labelled,
    pub criteria: Vec<Criterion>,
    pub trajectory: String,
    pub turn_end: String,
    pub earlier_turns: Vec<String>,
    pub expected_fires: bool,
}

pub(super) struct Checklist;

fn text_at<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    path.iter()
        .try_fold(value, |v, k| v.get(*k))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn stored_criteria(state: &Value) -> Option<Vec<Criterion>> {
    let map = state.get("criteria")?.as_object()?;
    (0..map.len())
        .map(|i| {
            let c = map.get(&key(i))?;
            let text = c.as_str().or_else(|| c.get("text")?.as_str())?;
            Some(Criterion {
                text: text.to_owned(),
                inferred: false,
                context: c
                    .get("their_words")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

fn ready(record: Labelled) -> Option<Ready> {
    let criteria = stored_criteria(&record.state)?;
    let fired = record
        .composed
        .as_ref()?
        .get("verdict")?
        .get("fires")?
        .as_bool()?;
    let earlier_turns = record
        .state
        .get("facts")
        .and_then(|f| f.get("earlier_turns"))
        .and_then(|t| serde_json::from_value(t.clone()).ok())
        .unwrap_or_default();
    Some(Ready {
        trajectory: text_at(&record.state, &["evidence", "trajectory"]).to_owned(),
        turn_end: text_at(&record.state, &["evidence", "turn_end"]).to_owned(),
        expected_fires: if record.right() { fired } else { !fired },
        earlier_turns,
        criteria,
        record,
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

impl ReplayPoint for Checklist {
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
        format!(
            "{} expected_fires={}",
            item.record.label, item.expected_fires
        )
    }

    fn prepare(item: &Ready) -> Prepared {
        let evidence = Evidence {
            trajectory: &item.trajectory,
            turn_end: &item.turn_end,
            earlier_turns: &item.earlier_turns,
        };
        Prepared {
            state: state(&item.criteria, &evidence),
            questions: questions(&item.criteria),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let checklist = compose(asked, &item.criteria)?;
        let fires = checklist.fires();
        let right = fires == item.expected_fires;
        Some(Judged {
            composed: json!({ "fires": fires, "expected_fires": item.expected_fires }),
            score: checklist.confidence(),
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
