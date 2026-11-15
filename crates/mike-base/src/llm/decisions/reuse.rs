use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use super::Answer;
use super::point::Asked;
use crate::store::persistence::PersistHandle;

const RECENT_SQL: &str = "SELECT id, answers
   FROM decisions.records
  WHERE point = $1
    AND question_set = $2
    AND created_at >= now() - ($4 * interval '1 second')
    AND mode IN ('on', 'shadow')
    AND outcome = 'ok'
    AND error IS NULL
    AND question_refusals = 0
    AND answers IS NOT NULL
    AND state = $3
  ORDER BY created_at DESC, id DESC
  LIMIT 1";

fn rebuilt(answers: Value) -> Option<Asked> {
    let parsed: BTreeMap<String, Answer> = serde_json::from_value(answers).ok()?;
    if parsed.is_empty() || parsed.values().any(Answer::is_refusal) {
        return None;
    }
    Some(Asked::answered(parsed))
}

pub async fn recent_answers(
    persist: &PersistHandle,
    point: &str,
    question_set: &str,
    state: &Value,
    within: Duration,
) -> Option<(i64, Asked)> {
    let row = crate::metrics::time_db("decision_recent_answers", async {
        sqlx::query_as::<_, (i64, Option<Value>)>(RECENT_SQL)
            .bind(point)
            .bind(question_set)
            .bind(state)
            .bind(within.as_secs_f64())
            .fetch_optional(persist.pool())
            .await
    })
    .await
    .map_err(|error| tracing::warn!(%error, point, "decisions: reuse lookup failed"))
    .ok()??;
    let asked = rebuilt(row.1?)?;
    Some((row.0, asked))
}

#[cfg(test)]
#[path = "reuse_tests.rs"]
mod reuse_tests;
