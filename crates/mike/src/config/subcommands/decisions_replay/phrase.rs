use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use crate::analysis::phrase_point::compose::{EXCUSE_AT, compose};
use crate::analysis::phrase_point::questions::{POINT, QUESTION_SET, key, questions};
use crate::llm::decisions::point::Asked;
use crate::store::persistence::PersistHandle;

pub(super) struct Ready {
    pub record: Labelled,
    pub count: usize,
    pub expected: Vec<bool>,
}

pub(super) struct Phrase;

fn expected_one(record: &Labelled, question: &str) -> Option<bool> {
    let named = record
        .evidence
        .get("expected")
        .and_then(|e| e.get(question))
        .and_then(Value::as_str);
    match named {
        Some(want) if !record.right() => Some(want == "yes"),
        _ => record.old_noul(question).map(|p| p >= EXCUSE_AT),
    }
}

fn ready(record: Labelled) -> Option<Ready> {
    let count = record.state.get("matches")?.as_object()?.len();
    let expected = (0..count)
        .map(|i| expected_one(&record, &key(i)))
        .collect::<Option<Vec<bool>>>()?;
    Some(Ready {
        record,
        count,
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

impl ReplayPoint for Phrase {
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
        format!("{} expected={:?}", item.record.label, item.expected)
    }

    fn prepare(item: &Ready) -> Prepared {
        Prepared {
            state: item.record.state.clone(),
            questions: questions(item.count),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let excuses = compose(asked, item.count)?;
        let right = excuses == item.expected;
        Some(Judged {
            composed: json!({ "excuses": excuses, "expected": item.expected }),
            score: if right { 1.0 } else { 0.0 },
            positive: Some(right),
            decision_right: Some(right),
            agrees_current: Some(item.record.right() == right),
        })
    }

    fn question_positive(item: &Ready, question: &str) -> Option<bool> {
        let index = (0..item.count).find(|i| key(*i) == question)?;
        item.expected.get(index).copied()
    }

    fn current_positive(item: &Ready, question: &str) -> Option<bool> {
        item.record.old_noul(question).map(|p| p >= EXCUSE_AT)
    }
}
