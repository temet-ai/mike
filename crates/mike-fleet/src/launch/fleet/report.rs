use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::acceptance::{authoritative, tasks_named};
use super::definition::{FleetDefinition, TaskDefinition};
use super::seated::Seated;
use super::status::NoteFacts;
use crate::change_record::ChangeEvent;
use crate::change_record::answers::CheckAfterLastChange;

pub const REPORT_KIND: &str = "report";
pub const DECISION_KIND: &str = "decision";
pub const REPORT_TAG: &str = "report";
pub const VERDICT_TAG: &str = "verdict";
pub const CHECK_PASSED: &str = "passed";
pub const CHECK_FAILED: &str = "failed";
pub const CHECK_NOT_YET: &str = "not checked at this commit yet";
pub const CHECK_RECORD_OFF: &str = "unknown: the change record is off";
pub const CHECK_NO_COMMIT: &str = "no commit by this seat for the task yet";
pub const NO_TESTS_CHANGED: &str = "none";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Exercised {
    pub command: String,
    pub exit: Option<i32>,
    pub output_ref: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ReportRequest {
    pub session: Option<Uuid>,
    pub task: String,
    pub slice: String,
    #[serde(deserialize_with = "criteria_input")]
    pub criteria: Vec<String>,
    pub summary: String,
    pub look_at: String,
    pub evidence: Vec<String>,
    #[serde(alias = "commit")]
    pub sha: Option<String>,
    pub interface: bool,
    pub exercised: Option<Exercised>,
    #[serde(deserialize_with = "list_or_none")]
    pub tests_changed: Option<Vec<String>>,
    pub unestablished: Option<Vec<String>>,
}

fn list_or_none<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input {
        List(Vec<String>),
        Text(String),
    }
    Ok(
        Option::<Input>::deserialize(deserializer)?.map(|input| match input {
            Input::List(values) => values,
            Input::Text(text) => vec![text],
        }),
    )
}

fn incomplete(field: &str, hint: &str) -> String {
    format!("fleet_report is incomplete: `{field}` is missing; {hint}")
}

fn require_exercised(exercised: Option<&Exercised>) -> Result<(), String> {
    let hint = "give {command, exit, output_ref}: the command you ran, its exit code, and where its output is kept";
    let exercised = exercised.ok_or_else(|| incomplete("exercised", hint))?;
    if exercised.command.trim().is_empty() {
        return Err(incomplete("exercised.command", hint));
    }
    if exercised.exit.is_none() {
        return Err(incomplete("exercised.exit", hint));
    }
    if exercised.output_ref.trim().is_empty() {
        return Err(incomplete("exercised.output_ref", hint));
    }
    Ok(())
}

fn require_tests_changed(named: Option<&[String]>) -> Result<(), String> {
    let hint = "list the test files you changed, or \"none\"";
    let named = named.filter(|list| !list.is_empty());
    let named = named.ok_or_else(|| incomplete("tests_changed", hint))?;
    if named.iter().any(|path| path.trim().is_empty()) {
        return Err(incomplete("tests_changed", hint));
    }
    if named.len() > 1 && named.iter().any(|path| path.trim() == NO_TESTS_CHANGED) {
        return Err(format!(
            "fleet_report is incomplete: `tests_changed` lists \"{NO_TESTS_CHANGED}\" beside paths; give the paths or \"{NO_TESTS_CHANGED}\""
        ));
    }
    Ok(())
}

pub fn require_typed(req: &ReportRequest) -> Result<(), String> {
    if req.sha.as_deref().is_none_or(|sha| sha.trim().is_empty()) {
        return Err(incomplete("commit", "name the commit you report"));
    }
    if req.criteria.is_empty() {
        return Err(incomplete(
            "criteria",
            "name the criterion ids the slice serves",
        ));
    }
    require_exercised(req.exercised.as_ref())?;
    require_tests_changed(req.tests_changed.as_deref())?;
    if req.unestablished.is_none() {
        return Err(incomplete(
            "unestablished",
            "list what you did not establish, or []",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct VerdictRequest {
    pub session: Option<Uuid>,
    pub task: String,
    #[serde(deserialize_with = "criteria_input")]
    pub criteria: Vec<String>,
    pub verdict: String,
    pub scope: String,
    pub reason: String,
    pub report_note: Option<Uuid>,
    pub regressions: Vec<super::regression::Regression>,
}

fn criteria_input<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input {
        List(Vec<String>),
        Text(String),
    }
    let values = match Input::deserialize(deserializer)? {
        Input::List(values) => values,
        Input::Text(text) => text.split(',').map(str::to_string).collect(),
    };
    let mut criteria = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            return Err(serde::de::Error::custom(
                "criteria must be nonempty ids, for example [\"C3\", \"C5\"] or \"C3, C5\"; omit criteria for all task criteria",
            ));
        }
        if !criteria.iter().any(|id| id == value) {
            criteria.push(value.to_string());
        }
    }
    Ok(criteria)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommitFacts {
    pub commit: Option<String>,
    pub check: String,
    pub checked_after_last_change: Option<CheckAfterLastChange>,
    pub brief_version: Option<i64>,
    pub check_command: String,
    pub proof: super::report_proof::ReportProof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    pub criteria: Vec<String>,
    pub verdict: &'static str,
    pub scope: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub task: String,
    pub seat: String,
    pub since: DateTime<Utc>,
    pub reports: usize,
    pub newest: Uuid,
    pub newest_at: DateTime<Utc>,
}

fn task_named<'a>(def: &'a FleetDefinition, id: &str) -> Result<&'a TaskDefinition, String> {
    def.tasks
        .get(id)
        .ok_or_else(|| format!("`{id}` is not a task of this fleet"))
}

fn criteria_of(id: &str, task: &TaskDefinition, named: &[String]) -> Result<Vec<String>, String> {
    if named.is_empty() {
        return Ok(task.criteria.clone());
    }
    match named.iter().find(|c| !task.criteria.contains(c)) {
        Some(c) => Err(format!(
            "`{c}` is not a criterion of task {id}; it serves {}",
            task.criteria.join(", ")
        )),
        None => Ok(named.to_vec()),
    }
}

pub fn acceptance_session(def: &FleetDefinition) -> Option<Uuid> {
    def.acceptance.session.as_deref()?.trim().parse().ok()
}

pub fn task_session(task: &TaskDefinition) -> Option<Uuid> {
    task.session.as_deref()?.trim().parse().ok()
}

pub fn check_report<'a>(
    def: &'a FleetDefinition,
    seated: &Seated,
    req: &ReportRequest,
) -> Result<(&'a TaskDefinition, Vec<String>), String> {
    let session = req
        .session
        .ok_or("pass your own session id as `session_id`")?;
    let task = task_named(def, &req.task)?;
    if !seated.holds(session, &req.task) {
        return Err(format!(
            "task {} is not held by session {session}; a seat reports only the task it holds",
            req.task
        ));
    }
    if req.slice.trim().is_empty() {
        return Err("name the slice you report (`slice`)".to_string());
    }
    require_typed(req)?;
    Ok((task, criteria_of(&req.task, task, &req.criteria)?))
}

fn verdict_word(word: &str) -> Result<&'static str, String> {
    match word.trim().to_lowercase().as_str() {
        "pass" => Ok("pass"),
        "fail" => Ok("fail"),
        other => Err(format!("`verdict` is `pass` or `fail`, not `{other}`")),
    }
}

fn scope_word(word: &str) -> Result<&'static str, String> {
    match word.trim().to_lowercase().as_str() {
        "slice" => Ok("slice"),
        "final" => Ok("final"),
        other => Err(format!("`scope` is `slice` or `final`, not `{other}`")),
    }
}

pub fn check_verdict<'a>(
    def: &'a FleetDefinition,
    req: &VerdictRequest,
) -> Result<(&'a TaskDefinition, Judged), String> {
    let session = req
        .session
        .ok_or("pass your own session id as `session_id`")?;
    if acceptance_session(def) != Some(session) {
        return Err(format!(
            "session {session} is not this fleet's acceptance seat; only acceptance records a verdict"
        ));
    }
    super::regression::validate(def, req)?;
    let task = task_named(def, &req.task)?;
    let verdict = verdict_word(&req.verdict)?;
    let scope = scope_word(&req.scope)?;
    if req.reason.trim().is_empty() {
        return Err("say why (`reason`): the seat reads it".to_string());
    }
    let criteria = criteria_of(&req.task, task, &req.criteria)?;
    if criteria.is_empty() {
        return Err(format!("task {} serves no criterion to judge", req.task));
    }
    Ok((
        task,
        Judged {
            criteria,
            verdict,
            scope,
        },
    ))
}

pub fn check_at_commit(events: &[ChangeEvent], sha: &str) -> &'static str {
    let found = events.iter().find(|event| {
        event.after_sha.as_deref() == Some(sha)
            || event.detail.get("commit").and_then(Value::as_str) == Some(sha)
    });
    match found.and_then(|event| event.detail.get("passed").and_then(Value::as_bool)) {
        Some(true) => CHECK_PASSED,
        Some(false) => CHECK_FAILED,
        None => CHECK_NOT_YET,
    }
}

pub fn report_title(req: &ReportRequest) -> String {
    format!("Report {}: {}", req.task, req.slice.trim())
}

pub fn report_body(req: &ReportRequest) -> String {
    let (summary, look_at) = (req.summary.trim(), req.look_at.trim());
    match (summary.is_empty(), look_at.is_empty()) {
        (false, false) => format!("{summary}\n\nWhat to look at: {look_at}"),
        (false, true) => summary.to_string(),
        _ => look_at.to_string(),
    }
}

pub fn report_data(req: &ReportRequest, criteria: &[String], facts: &CommitFacts) -> Value {
    json!({
        "task": req.task,
        "slice": req.slice.trim(),
        "criteria": criteria,
        "commit": facts.commit,
        "check": facts.check,
        "checked_after_last_change": facts.checked_after_last_change,
        "brief_version": facts.brief_version,
        "check_command": facts.check_command,
        "proof": facts.proof,
        "evidence": req.evidence,
        "interface": req.interface,
        "exercised": req.exercised,
        "tests_changed": req.tests_changed,
        "unestablished": req.unestablished,
    })
}

fn after_words(after: Option<&CheckAfterLastChange>) -> String {
    match after {
        Some(a) if a.ran => format!(
            "yes, {}",
            a.verdict.map_or("with no verdict", |v| match v {
                crate::change_record::answers::CheckVerdict::Passed => "passed",
                crate::change_record::answers::CheckVerdict::Failed => "failed",
            })
        ),
        Some(_) => "no".to_string(),
        None => "unknown".to_string(),
    }
}

fn listed(items: &[String]) -> String {
    if items.is_empty() {
        "none named".to_string()
    } else {
        items.join(", ")
    }
}

pub fn acceptance_mail(
    seat: &str,
    req: &ReportRequest,
    criteria: &[String],
    facts: &CommitFacts,
    note: Uuid,
) -> String {
    format!(
        "[mike report] Task {task} ({seat}), slice \"{slice}\", criteria {criteria}.\nReport note: {note}.\nCommit: {commit}.\nCheck at that commit: {check}.\nCheck command: {command}.\nThe check ran after the seat's last change: {after}.\nBrief version the seat read: {brief}.\nEvidence: {evidence}.\n{proof}\nWhat to look at is in the note's body. Answer with fleet_verdict (task {task}, report_note {note}).",
        task = req.task,
        slice = req.slice.trim(),
        criteria = listed(criteria),
        commit = facts.commit.as_deref().unwrap_or("none"),
        check = facts.check,
        command = facts.check_command,
        proof = proof_notice(&facts.proof),
        after = after_words(facts.checked_after_last_change.as_ref()),
        brief = facts
            .brief_version
            .map_or_else(|| "unknown".to_string(), |v| v.to_string()),
        evidence = listed(&req.evidence),
    )
}

fn proof_notice(proof: &super::report_proof::ReportProof) -> String {
    format!(
        "Recorded test/lint receipts: {}. Read data.proof in the report note for commands, validity and full-output references. Receipt query limit reached: {}. Receipt lookup: {}.",
        proof.receipts.len(),
        proof.limit_reached,
        if proof.unavailable.is_some() {
            "unavailable; see note"
        } else {
            "available"
        },
    )
}

pub fn decision_title(task: &str, judged: &Judged, reason: &str) -> String {
    format!(
        "{} {task} {} ({}): {}",
        judged.verdict.to_uppercase(),
        judged.criteria.join(" "),
        judged.scope,
        reason.trim().lines().next().unwrap_or_default()
    )
}

pub fn decision_data(
    task: &str,
    judged: &Judged,
    (reason, report_note, brief_version): (&str, Option<Uuid>, Option<i64>),
) -> Value {
    json!({
        "verdict": judged.verdict,
        "task": task,
        "criteria": judged.criteria,
        "scope": judged.scope,
        "reason": reason.trim(),
        "report_note": report_note,
        "brief_version": brief_version,
    })
}

pub fn seat_mail(task: &str, judged: &Judged, reason: &str, report_note: Option<Uuid>) -> String {
    let about = report_note.map_or_else(String::new, |n| format!(" on report note {n}"));
    let next = if judged.verdict == "fail" {
        " Fix it before anything new, commit, and report again with fleet_report."
    } else {
        ""
    };
    format!(
        "[mike verdict] {} for task {task}, criteria {}, scope {}{about}.\nReason: {}{next}",
        judged.verdict.to_uppercase(),
        judged.criteria.join(", "),
        judged.scope,
        reason.trim()
    )
}

pub fn newest_commit_of(notes: &[(Value, DateTime<Utc>)], task: &str) -> Option<String> {
    notes
        .iter()
        .filter(|(data, _)| super::commit_note::names_task(data, task))
        .filter(|(data, _)| data.get("sha").and_then(Value::as_str).is_some())
        .max_by_key(|(_, at)| *at)
        .and_then(|(data, _)| data.get("sha").and_then(Value::as_str))
        .map(str::to_string)
}

fn answered_at(def: &FleetDefinition, notes: &[NoteFacts]) -> BTreeMap<String, DateTime<Utc>> {
    let mut answered: BTreeMap<String, DateTime<Utc>> = BTreeMap::new();
    for note in notes
        .iter()
        .filter(|n| n.kind == DECISION_KIND && authoritative(def, n))
    {
        for task in tasks_named(&note.data) {
            let at = answered.entry(task).or_insert(note.updated_at);
            *at = (*at).max(note.updated_at);
        }
    }
    answered
}

pub fn waiting(def: &FleetDefinition, notes: &[NoteFacts]) -> Vec<Waiting> {
    let answered = answered_at(def, notes);
    let mut by_task: BTreeMap<String, Waiting> = BTreeMap::new();
    for note in notes.iter().filter(|n| n.kind == REPORT_KIND) {
        let Some(id) = note.data.get("task").and_then(Value::as_str) else {
            continue;
        };
        let Some(task) = def.tasks.get(id) else {
            continue;
        };
        if answered.get(id).is_some_and(|at| *at >= note.created_at) {
            continue;
        }
        let entry = by_task.entry(id.to_string()).or_insert_with(|| Waiting {
            task: id.to_string(),
            seat: task.seat.clone().unwrap_or_default(),
            since: note.created_at,
            reports: 0,
            newest: note.note_uuid,
            newest_at: note.created_at,
        });
        if note.created_at > entry.newest_at {
            entry.newest = note.note_uuid;
            entry.newest_at = note.created_at;
        }
        entry.reports += 1;
        entry.since = entry.since.min(note.created_at);
    }
    let mut waiting: Vec<Waiting> = by_task.into_values().collect();
    waiting.sort_by_key(|w| w.since);
    waiting
}
