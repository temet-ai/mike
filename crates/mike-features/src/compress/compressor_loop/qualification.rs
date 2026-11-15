use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::canonical::canonical_json;

const SCHEMA: &str = "compressor-qualification-v1";
const MAX_AGE_SECONDS: f64 = 7.0 * 86400.0;
const FAMILIES: [&str; 3] = ["documentation", "configuration", "test-output"];
const ARMS: [&str; 2] = ["original", "compact"];
const PAIR_HASHES: [&str; 4] = [
    "task_hash",
    "ground_truth_hash",
    "artifact_hash",
    "model_config_hash",
];

pub(super) fn policy_hash(policy: &Value) -> String {
    hex::encode(Sha256::digest(canonical_json(policy).as_bytes()))
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(fields)) => !fields.is_empty(),
    }
}

fn parse_time(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

fn text_bytes(content: Option<&Value>) -> Option<usize> {
    match content? {
        Value::String(text) => Some(text.len()),
        Value::Array(blocks) => Some(
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .map(str::len)
                .sum(),
        ),
        _ => None,
    }
}

fn unique_ids(items: &[Value], field: &str) -> Option<HashSet<String>> {
    let ids: HashSet<String> = items
        .iter()
        .map(|item| item.get(field).map(Value::to_string))
        .collect::<Option<_>>()?;
    (ids.len() == items.len()).then_some(ids)
}

struct TrialScore {
    correct: bool,
    actual_total_tokens: Option<i64>,
    missing_results: usize,
}

fn model_tokens(turns: &[Value]) -> Option<i64> {
    let mut total = 0i64;
    for turn in turns {
        let input = turn.get("input_tokens").and_then(Value::as_i64)?;
        let output = turn.get("output_tokens").and_then(Value::as_i64)?;
        if input < 0 || output < 0 {
            return None;
        }
        total += input + output;
    }
    (!turns.is_empty()).then_some(total)
}

fn score(run: &Value) -> Option<TrialScore> {
    if run.get("execution_kind").and_then(Value::as_str) != Some("actual")
        || !truthy(run.get("execution_ref"))
        || !truthy(run.get("verdict_ref"))
    {
        return None;
    }
    let correct = run.get("correct").and_then(Value::as_bool)?;
    let calls = run.get("calls")?.as_array()?;
    let call_ids = unique_ids(calls, "id")?;
    let started = parse_time(run.get("started_at")?)?;
    let finished = parse_time(run.get("finished_at")?)?;
    if finished < started {
        return None;
    }
    let turns = optional_array(run, "model_turns")?;
    unique_ids(turns, "id")?;
    let facts = optional_array(run, "required_facts")?;
    unique_ids(facts, "id")?;
    for fact in facts {
        if let Some(found) = fact.get("first_call_id").filter(|found| !found.is_null())
            && !call_ids.contains(&found.to_string())
        {
            return None;
        }
    }
    let missing_results = calls
        .iter()
        .filter(|call| text_bytes(call.get("content")).is_none())
        .count();
    Some(TrialScore {
        correct,
        actual_total_tokens: model_tokens(turns),
        missing_results,
    })
}

fn optional_array<'a>(run: &'a Value, field: &str) -> Option<&'a [Value]> {
    match run.get(field) {
        None => Some(&[]),
        Some(value) => value.as_array().map(Vec::as_slice),
    }
}

fn fact_identities(run: &Value) -> Option<HashSet<String>> {
    optional_array(run, "required_facts")?
        .iter()
        .map(|fact| fact.get("id").map(Value::to_string))
        .collect()
}

struct Compared {
    original: TrialScore,
    compact: TrialScore,
}

fn compare_pair(pair: &Value) -> Option<Compared> {
    let original = pair.get("original")?;
    let compact = pair.get("compact")?;
    for field in PAIR_HASHES {
        if !truthy(original.get(field)) || original.get(field) != compact.get(field) {
            return None;
        }
    }
    if fact_identities(original)? != fact_identities(compact)? {
        return None;
    }
    pair.get("task_id")?;
    Some(Compared {
        original: score(original)?,
        compact: score(compact)?,
    })
}

fn complete_run(pair: &Value, arm: &str) -> bool {
    let Some(run) = pair.get(arm) else {
        return false;
    };
    truthy(run.get("calls"))
        && truthy(run.get("model_turns"))
        && truthy(run.get("model_config_hash"))
        && truthy(run.get("activation_ref"))
        && truthy(run.get("constraint_verdict_ref"))
}

fn recovery_passed(evidence: &Value) -> Option<bool> {
    let recovery = evidence.get("recovery")?;
    Some(
        recovery.get("passed") == Some(&Value::Bool(true))
            && recovery.get("weakened_control_failed") == Some(&Value::Bool(true))
            && truthy(recovery.get("evidence_ref")),
    )
}

fn families_covered(pairs: &[Value]) -> bool {
    FAMILIES.iter().all(|family| {
        let tasks: HashSet<String> = pairs
            .iter()
            .filter(|pair| pair.get("family").and_then(Value::as_str) == Some(*family))
            .filter_map(|pair| pair.get("task_id").map(Value::to_string))
            .collect();
        tasks.len() >= 3
    })
}

fn qualifies_inner(evidence: &Value, policy: &Value, now: DateTime<Utc>) -> Option<bool> {
    if evidence.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        return Some(false);
    }
    if evidence.get("policy_sha256").and_then(Value::as_str) != Some(policy_hash(policy).as_str()) {
        return Some(false);
    }
    if truthy(policy.get("implementation")) && !truthy(policy.get("build_sha")) {
        return Some(false);
    }
    let completed = parse_time(evidence.get("completed_at")?)?;
    let age = (now - completed).num_microseconds()? as f64 / 1_000_000.0;
    if !(0.0..=MAX_AGE_SECONDS).contains(&age) || !recovery_passed(evidence)? {
        return Some(false);
    }
    let pairs = evidence.get("pairs")?.as_array()?;
    let scored: Vec<Compared> = pairs.iter().map(compare_pair).collect::<Option<_>>()?;
    let solid = |score: &TrialScore| {
        score.correct && score.actual_total_tokens.is_some() && score.missing_results == 0
    };
    if scored.is_empty()
        || !scored
            .iter()
            .all(|c| solid(&c.original) && solid(&c.compact))
    {
        return Some(false);
    }
    if !families_covered(pairs) {
        return Some(false);
    }
    Some(
        pairs
            .iter()
            .all(|pair| ARMS.iter().all(|arm| complete_run(pair, arm))),
    )
}

pub(super) fn qualifies(evidence: Option<&Value>, policy: &Value, now: DateTime<Utc>) -> bool {
    evidence
        .filter(|found| found.is_object())
        .and_then(|found| qualifies_inner(found, policy, now))
        .unwrap_or(false)
}
