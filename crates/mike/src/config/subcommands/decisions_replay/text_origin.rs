use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::labelled::{self, Labelled};
use super::point::{Judged, Prepared, ReplayPoint};
use crate::llm::decisions::point::Asked;
use crate::llm::text_origin::{
    self, APPLY_AT, POINT, QUESTION_SET, Unmatched, compose, current_text, prior_context, signals,
};
use crate::store::persistence::PersistHandle;

const KEPT: &str = "operator";

pub(super) struct Ready {
    pub record: Labelled,
    pub input: Unmatched,
    pub expected: String,
    pub before: String,
}

pub(super) struct TextOrigin;

fn decided_before(record: &Labelled) -> String {
    if let Some(decided) = record.evidence_str("decided") {
        return decided.to_owned();
    }
    let composed = record.composed.as_ref();
    let score = composed
        .and_then(|c| c.get("score"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let origin = composed
        .and_then(|c| c.get("origin"))
        .and_then(Value::as_str);
    match origin {
        Some(origin) if score >= APPLY_AT => origin.to_owned(),
        _ => KEPT.to_owned(),
    }
}

fn expected(record: &Labelled, before: &str) -> Option<String> {
    if record.right() {
        return Some(before.to_owned());
    }
    record
        .expected_for(text_origin::questions::ORIGIN)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

async fn ready(persist: &PersistHandle, record: Labelled) -> Option<Ready> {
    let request_id = u64::try_from(record.request_id?).ok()?;
    let body = persist
        .fetch_request_body_by_id(request_id)
        .await
        .ok()
        .flatten()?
        .bytes;
    let text = current_text(&body);
    let prior = prior_context(&body);
    let before = decided_before(&record);
    let expected = expected(&record, &before)?;
    let input = Unmatched {
        request_id: record.request_id?,
        session_db_id: record.session_id.unwrap_or_default(),
        last_user_message: text.clone(),
        previous_user_message: prior.user_message,
        previous_assistant_reply: prior.assistant_reply,
        author: String::new(),
        has_agent_id: false,
        signals: signals(&body, &text),
        text,
    };
    Some(Ready {
        record,
        input,
        expected,
        before,
    })
}

pub(super) async fn load(
    persist: &PersistHandle,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<(Vec<Ready>, usize)> {
    let records = labelled::load(persist, POINT, since, limit).await?;
    let total = records.len();
    let mut out = Vec::with_capacity(total);
    for record in records {
        if let Some(item) = ready(persist, record).await {
            out.push(item);
        }
    }
    let skipped = total - out.len();
    Ok((out, skipped))
}

impl ReplayPoint for TextOrigin {
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
            state: text_origin::state::state(&item.input),
            questions: text_origin::questions::questions(),
        }
    }

    fn compose(item: &Ready, asked: &Asked) -> Option<Judged> {
        let verdict = compose(asked)?;
        let decided = if verdict.applies() {
            verdict.origin.key()
        } else {
            KEPT
        };
        let right = decided == item.expected;
        Some(Judged {
            composed: json!({
                "origin": verdict.origin.key(),
                "probability": verdict.probability,
                "decided": decided,
                "expected": item.expected,
            }),
            score: verdict.probability,
            positive: Some(right),
            decision_right: Some(right),
            agrees_current: Some(decided == item.before),
        })
    }

    fn question_positive(_: &Ready, _: &str) -> Option<bool> {
        None
    }

    fn current_positive(_: &Ready, _: &str) -> Option<bool> {
        None
    }
}
