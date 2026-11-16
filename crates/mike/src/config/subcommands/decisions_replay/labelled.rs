use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;

use crate::store::persistence::PersistHandle;

pub(super) const SOURCE: &str = "claude_review";

const SQL: &str = "SELECT r.id AS record_id, r.created_at, r.state, r.composed, \
            r.answers, r.request_id, r.session_id, r.operator_turn_id, o.label, o.evidence \
       FROM decisions.outcomes o \
       JOIN decisions.records r ON r.id = o.record_id \
      WHERE r.point = $1 AND o.label_source = $2 AND o.label IN ('right', 'wrong') \
        AND r.created_at >= $3 \
      ORDER BY r.id DESC \
      LIMIT $4";

#[derive(Debug, Clone, FromRow)]
pub(super) struct Labelled {
    pub record_id: i64,
    pub created_at: DateTime<Utc>,
    pub state: Value,
    pub composed: Option<Value>,
    pub answers: Option<Value>,
    pub request_id: Option<i64>,
    pub session_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub label: String,
    pub evidence: Value,
}

impl Labelled {
    pub(super) fn right(&self) -> bool {
        self.label == "right"
    }

    pub(super) fn evidence_str(&self, key: &str) -> Option<&str> {
        self.evidence.get(key).and_then(Value::as_str)
    }

    pub(super) fn failed_question(&self) -> Option<&str> {
        (!self.right())
            .then(|| self.evidence_str("question"))
            .flatten()
    }

    pub(super) fn expected_for(&self, question: &str) -> Option<&Value> {
        if self.failed_question() != Some(question) {
            return None;
        }
        let expected = self.evidence.get("expected")?;
        match expected {
            Value::Object(map) => map.get(question).or(Some(expected)),
            other => Some(other),
        }
    }

    pub(super) fn old_choice(&self, question: &str) -> Option<&str> {
        self.answers
            .as_ref()?
            .get(question)?
            .get("choice")
            .and_then(Value::as_str)
    }

    pub(super) fn old_noul(&self, question: &str) -> Option<f64> {
        self.answers.as_ref()?.get(question)?.get("noul")?.as_f64()
    }
}

pub(super) async fn load(
    persist: &PersistHandle,
    point: &str,
    since: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<Labelled>> {
    let rows = sqlx::query_as::<_, Labelled>(SQL)
        .bind(point)
        .bind(SOURCE)
        .bind(since)
        .bind(limit)
        .fetch_all(persist.pool())
        .await?;
    let right = rows.iter().filter(|r| r.right()).count();
    println!(
        "{point}: before, {right} of {} labelled records were right ({SOURCE})",
        rows.len()
    );
    Ok(rows)
}
