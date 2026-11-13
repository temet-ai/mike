use crate::store::persistence::fleet_repo::FleetPersist as _;
use serde_json::{Value, json};

use crate::store::persistence::fleet::{
    FleetRow, RunMeasures, RunPatterns, SeatMeasure, TokenMeasure,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub const MEASURE_VERSION: &str = "run-record-v3";

const CONTROL_KINDS: [&str; 3] = ["coordinator", "acceptance", "observer"];

const UNMEASURED: [&str; 8] = [
    "check_timeout_minutes",
    "checks_run",
    "checks_reused",
    "builds_by_concurrency",
    "tool_errors_per_seat",
    "results_reused",
    "findings_read_by_another_seat",
    "hand_patches_and_stops",
];

pub const GIT_READ: &str = r"(^|[;&|(]\s*|\s)git\s+(-C\s+\S+\s+)?(--no-pager\s+)?(status|log|diff|show|rev-parse|ls-files|merge-base|fetch|branch|ls-tree|cat-file|rev-list|ls-remote|for-each-ref)\M";
pub const GIT_WRITE: &str = r"(^|[;&|(]\s*|\s)git\s+(-C\s+\S+\s+)?(add|commit|push|pull|checkout|reset|stash|merge|rebase|apply|rm|mv|cherry-pick|restore|switch|clone|worktree|update-index|update-ref|read-tree|commit-tree|tag|init|clean)\M";

pub fn operation_pattern(def: &super::definition::FleetDefinition) -> String {
    let names: Vec<String> = def
        .execution
        .operations
        .values()
        .map(|op| op.command.trim())
        .filter(|command| !command.is_empty())
        .map(regex::escape)
        .collect();
    if names.is_empty() {
        return "a^".into();
    }
    format!(r"^\s*({})\s*$", names.join("|"))
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        return sorted.get(mid).copied();
    }
    let low = sorted.get(mid.checked_sub(1)?)?;
    let high = sorted.get(mid)?;
    Some((low + high) / 2.0)
}

fn longest(values: &[f64]) -> Option<f64> {
    values.iter().copied().max_by(f64::total_cmp)
}

fn union_of(seats: &[SeatMeasure], names: impl Fn(&SeatMeasure) -> &[String]) -> Vec<String> {
    let mut all: Vec<String> = seats.iter().flat_map(|s| names(s).to_vec()).collect();
    all.sort();
    all.dedup();
    all
}

fn per_100(part: i64, whole: i64) -> Option<f64> {
    (whole > 0).then(|| round1(100.0 * part as f64 / whole as f64))
}

fn control_model_minutes(m: &RunMeasures) -> f64 {
    m.seats
        .iter()
        .filter(|s| CONTROL_KINDS.contains(&s.kind.as_str()))
        .map(|s| s.working_minutes)
        .sum()
}

fn fault_minutes(m: &RunMeasures) -> Value {
    let parts = [
        ("no_seat", m.timing.no_seat_minutes),
        ("relaunch_gap", m.timing.relaunch_gap_minutes),
        ("report_to_verdict_wait", m.reports.late_minutes),
        ("resource_wait", 0.0),
        ("check_timeout", 0.0),
        ("stop", m.timing.stop_minutes),
        ("control_seat_model_time", control_model_minutes(m)),
    ];
    let mut fields: serde_json::Map<String, Value> = parts
        .iter()
        .map(|(name, minutes)| ((*name).to_string(), json!(round1(*minutes))))
        .collect();
    let total: f64 = parts.iter().map(|(_, minutes)| minutes).sum();
    fields.insert("total".into(), json!(round1(total)));
    Value::Object(fields)
}

fn token_total(t: &TokenMeasure) -> i64 {
    t.input_tokens + t.output_tokens + t.cache_read_tokens + t.cache_write_tokens
}

fn tokens_per_accepted_task(m: &RunMeasures) -> Value {
    let all: i64 = m.tokens.iter().map(token_total).sum();
    let accepted = m.timing.task_count;
    let by_kind: serde_json::Map<String, Value> = m
        .tokens
        .iter()
        .map(|t| {
            (
                t.kind.clone(),
                json!({
                    "requests": t.requests,
                    "input_tokens": t.input_tokens,
                    "output_tokens": t.output_tokens,
                    "cache_read_tokens": t.cache_read_tokens,
                    "cache_write_tokens": t.cache_write_tokens,
                    "total_tokens": token_total(t),
                }),
            )
        })
        .collect();
    json!({
        "total": (accepted > 0).then(|| round1(all as f64 / accepted as f64)),
        "all_tokens": all,
        "accepted_tasks": accepted,
        "accepted_basis": "tasks that had a worker seat",
        "by_seat_kind": by_kind,
    })
}

fn manual_interventions(m: &RunMeasures) -> Value {
    let turns = m.interventions.operator_turns;
    let starts = m.timing.hand_starts;
    let answers = m.interventions.operator_answers;
    json!({
        "total": turns + starts + answers,
        "operator_turns": turns,
        "mail_driven_turns": m.interventions.mail_driven_turns,
        "hand_starts": starts,
        "needs_you_answers": answers,
        "hand_patches": 0,
        "hand_stops": 0,
    })
}

fn context_replacements(m: &RunMeasures) -> usize {
    let control: Vec<&str> = m
        .seats
        .iter()
        .map(|s| s.kind.as_str())
        .filter(|kind| CONTROL_KINDS.contains(kind))
        .collect();
    let mut kinds = control.clone();
    kinds.sort_unstable();
    kinds.dedup();
    control.len() - kinds.len()
}

fn counts(m: &RunMeasures) -> Value {
    let tasks = m.timing.task_count;
    let unread: i64 = m.mail.iter().map(|p| p.messages - p.acknowledged).sum();
    json!({
        "seats_per_task": (tasks > 0).then(|| round1(m.timing.worker_launches as f64 / tasks as f64)),
        "judge_rounds_per_task": (tasks > 0).then(|| round1(m.interventions.reports_filed as f64 / tasks as f64)),
        "relaunches": (m.timing.worker_launches - tasks).max(0),
        "context_replacements": context_replacements(m),
        "reports_filed": m.interventions.reports_filed,
        "verdict_latency_median_minutes": m.reports.median_minutes,
        "unread_messages_at_close": unread,
        "problems_opened": 0,
        "problems_decided": 0,
    })
}

pub fn summarise(m: &RunMeasures) -> Value {
    let workers: Vec<&SeatMeasure> = m.seats.iter().filter(|s| s.kind == "worker").collect();
    let first_commits: Vec<f64> = workers
        .iter()
        .filter_map(|s| s.minutes_to_first_commit)
        .collect();
    let worker_calls: i64 = workers.iter().map(|s| s.tool_calls).sum();
    let worker_reads: i64 = workers.iter().map(|s| s.git_reads).sum();
    json!({
        "measure_version": MEASURE_VERSION,
        "time_to_done_minutes": m.timing.time_to_done_minutes,
        "fault_minutes": fault_minutes(m),
        "tokens_per_accepted_task": tokens_per_accepted_task(m),
        "manual_interventions": manual_interventions(m),
        "counts": counts(m),
        "unmeasured": UNMEASURED,
        "seat_count": m.seats.len(),
        "worker_count": workers.len(),
        "working_minutes": round1(m.seats.iter().map(|s| s.working_minutes).sum()),
        "waiting_minutes": round1(m.seats.iter().map(|s| s.waiting_minutes).sum()),
        "first_commit_median_minutes": median(&first_commits),
        "first_commit_longest_minutes": longest(&first_commits),
        "reports": m.reports.reports,
        "reports_answered": m.reports.answered,
        "report_to_verdict_median_minutes": m.reports.median_minutes,
        "report_to_verdict_longest_minutes": m.reports.longest_minutes,
        "worker_tool_calls": worker_calls,
        "worker_git_reads": worker_reads,
        "git_reads_per_100_calls": per_100(worker_reads, worker_calls),
        "operation_commands": m.operations.commands,
        "operation_repeats_across_seats": m.operations.repeats,
        "direct_operation_bypasses": m.operations.commands,
        "operation_adoption_action": if m.operations.commands > 0 { "Route exact declared commands through fleet_check to share admission, completion and eligible receipts." } else { "No exact declared operation bypass observed; wrapped or undeclared commands are outside this measure." },
        "operation_coverage": "exact declared commands observed in session tools; managed receipt executions are recorded separately in tree_checks",
        "questions_asked": m.questions.asked,
        "questions_answered": m.questions.answered,
        "forks": m.forks.forks,
        "forks_waited": m.forks.waited,
        "seats": m.seats,
        "mail_by_pair": m.mail,
        "tools_offered": union_of(&m.seats, |s| &s.offered),
        "tools_used": union_of(&m.seats, |s| &s.used),
    })
}

async fn measured_document(
    persist: &PersistHandle,
    row: &FleetRow,
    run_id: &str,
) -> Result<Value, PersistError> {
    let def = super::definition::FleetDefinition::from_value(&row.definition).unwrap_or_default();
    let operations = operation_pattern(&def);
    let patterns = RunPatterns {
        operations: &operations,
        git_read: GIT_READ,
        git_write: GIT_WRITE,
    };
    let measures = persist.fleet().run_measures(run_id, patterns).await?;
    let mut document = summarise(&measures);
    let timings = super::build_timing::read_run(&super::seat_settings::timings_dir(&def), run_id);
    super::build_waits::add_to_record(&mut document, super::build_waits::build_waits(&timings));
    Ok(document)
}

pub async fn write(
    persist: &PersistHandle,
    row: &FleetRow,
    run_id: &str,
) -> Result<bool, PersistError> {
    let document = measured_document(persist, row, run_id).await?;
    persist
        .fleet()
        .insert_run_record(row.fleet_uuid, run_id, &document)
        .await
}

pub async fn recompute(
    persist: &PersistHandle,
    row: &FleetRow,
    run_id: &str,
) -> Result<bool, PersistError> {
    let document = measured_document(persist, row, run_id).await?;
    persist
        .fleet()
        .replace_run_record(row.fleet_uuid, run_id, &document)
        .await
}

pub fn difference(record: &Value, against: &Value) -> Value {
    match (record, against) {
        (Value::Number(now), Value::Number(before)) => match (now.as_f64(), before.as_f64()) {
            (Some(now), Some(before)) => json!(round1(now - before)),
            _ => Value::Null,
        },
        (Value::Object(now), Value::Object(before)) => Value::Object(
            now.iter()
                .filter_map(|(key, value)| {
                    let delta = difference(value, before.get(key)?);
                    (!delta.is_null()).then(|| (key.clone(), delta))
                })
                .collect(),
        ),
        _ => Value::Null,
    }
}

fn say_written(slug: &str, run_id: &str, fresh: bool) {
    tracing::info!(fleet = %slug, run = %run_id, fresh, "fleet: the run has its record");
}

fn say_unwritten(run_id: &str, err: &PersistError) {
    tracing::warn!(error = %err, run = %run_id, "fleet: the run record could not be written");
}

pub async fn close_record(persist: &PersistHandle, row: &FleetRow, run_id: &str) {
    match write(persist, row, run_id).await {
        Ok(fresh) => say_written(&row.slug, run_id, fresh),
        Err(err) => say_unwritten(run_id, &err),
    }
}
