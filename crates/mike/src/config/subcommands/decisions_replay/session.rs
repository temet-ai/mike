use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::json;

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use crate::judge_worker::session_poll::jev::compose::compose;
use crate::judge_worker::session_poll::jev::questions::{OUTCOME, POINT, QUESTION_SET, questions};
use crate::llm::decisions::point::Asked;
use crate::store::persistence::PersistHandle;

pub(super) struct Ready {
    pub record: Labelled,
    pub expected: String,
}

pub(super) struct Session;

fn stored_outcome(record: &Labelled) -> Option<&str> {
    record
        .composed
        .as_ref()?
        .get("verdict")?
        .get("outcome")?
        .as_str()
}

fn ready(record: Labelled) -> Option<Ready> {
    let expected = if record.right() {
        stored_outcome(&record)?.to_owned()
    } else {
        record.expected_for(OUTCOME)?.as_str()?.to_owned()
    };
    Some(Ready { record, expected })
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

impl ReplayPoint for Session {
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
        format!("{} expected={}", item.record.label, item.expected)
    }

    fn prepare(item: &Ready) -> Prepared {
        Prepared {
            state: item.record.state.clone(),
            questions: questions(),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let decision = compose(asked)?;
        let right = decision.outcome == item.expected;
        Some(Judged {
            composed: json!({ "outcome": decision.outcome, "expected": item.expected }),
            score: decision.confidence,
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
