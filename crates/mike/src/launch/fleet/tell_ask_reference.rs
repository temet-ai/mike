use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::checks::normal;
use super::commit::covers;
use super::definition::{FleetDefinition, TaskDefinition};
use super::status::FleetStatus;
use super::tell::Notice;
use super::tell_reference::{quiet_seats, reference};
use crate::store::persistence::PersistHandle;

const READING_TOOLS: [&str; 3] = ["Read", "Grep", "Glob"];
const INSTRUCTION_FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];
const INSTRUCTION_FOLDER: &str = ".claude";
const LOOK_BACK_MINUTES: i64 = 5;
const USES_READ: i64 = 500;
const FAIL: &str = "fail";
const RECENT_MINUTES: i64 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRead {
    pub session: String,
    pub tool: String,
    pub input: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub seat: String,
    pub session: Uuid,
    pub tool: String,
    pub path: Option<String>,
}

fn read_key(seat: &str) -> String {
    format!("ask-reference-read:{seat}")
}

fn seated(def: &FleetDefinition) -> Vec<(&String, &TaskDefinition, String, Uuid)> {
    def.tasks
        .iter()
        .filter_map(|(id, task)| {
            let seat = task.seat.clone()?;
            let session = task.session.as_deref()?.trim().parse().ok()?;
            Some((id, task, seat, session))
        })
        .collect()
}

fn in_repo(repo: &str, path: &str) -> Option<String> {
    let root = repo.trim_end_matches('/');
    let inside = match path.strip_prefix(root) {
        Some("") => Some(""),
        Some(rest) => rest.strip_prefix('/'),
        None if !path.starts_with('/') => Some(path),
        None => None,
    }?;
    Some(normal(inside.trim_start_matches("./")))
}

fn instructions(path: &str) -> bool {
    path == INSTRUCTION_FOLDER
        || path.starts_with(&format!("{INSTRUCTION_FOLDER}/"))
        || INSTRUCTION_FILES
            .iter()
            .any(|name| path == *name || path.ends_with(&format!("/{name}")))
}

fn own(task: &TaskDefinition, path: &str) -> bool {
    let files: Vec<String> = task
        .files
        .iter()
        .chain(&task.new_files)
        .map(|f| normal(f))
        .filter(|f| !f.is_empty())
        .collect();
    covers(&files, path)
}

fn target(use_: &ToolRead) -> Option<Option<&str>> {
    let input = use_.input.as_ref();
    let field = |name: &str| input.and_then(|i| i.get(name)).and_then(Value::as_str);
    match use_.tool.as_str() {
        "Read" => field("file_path").map(Some),
        "Grep" | "Glob" => Some(field("path")),
        _ => None,
    }
}

pub fn readings(def: &FleetDefinition, uses: &[ToolRead]) -> Vec<Reading> {
    let workers = seated(def);
    uses.iter()
        .filter_map(|use_| {
            let (_, task, seat, session) = workers
                .iter()
                .find(|(_, _, _, s)| s.to_string().eq_ignore_ascii_case(use_.session.trim()))?;
            let path = match target(use_)? {
                Some(raw) => Some(in_repo(&def.repo, raw)?).filter(|p| !p.is_empty()),
                None => None,
            };
            let outside = path
                .as_deref()
                .is_none_or(|p| !own(task, p) && !instructions(p));
            outside.then(|| Reading {
                seat: seat.clone(),
                session: *session,
                tool: use_.tool.clone(),
                path,
            })
        })
        .collect()
}

pub fn holds(def: &FleetDefinition) -> String {
    let template = def
        .reference
        .discipline
        .as_deref()
        .map(|d| format!("the {d} template's reading of the project"))
        .unwrap_or_else(|| "the template's reading of the project".to_string());
    format!(
        "what every worker started with (the run's rules, {template} and the fleet definition) and reads the tree as it is now before it answers"
    )
}

fn doing(reading: &Reading) -> String {
    match (reading.tool.as_str(), reading.path.as_deref()) {
        ("Read", Some(path)) => format!("reading `{path}`"),
        (_, Some(path)) => format!("searching `{path}`"),
        _ => "searching the whole tree".to_string(),
    }
}

pub fn on_reading(
    def: &FleetDefinition,
    readings: &[Reading],
    said: impl Fn(&str) -> bool,
) -> Vec<Notice> {
    let Some(asked) = reference(def) else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    readings
        .iter()
        .filter(|r| r.session != asked && !said(&read_key(&r.seat)))
        .filter(|r| seen.insert(r.seat.clone()))
        .map(|r| Notice {
            to: r.session,
            key: read_key(&r.seat),
            covers: Vec::new(),
            body: format!(
                "[mike reference] You are {}, outside your task's files: the reference seat (session {asked}) holds {} and answers where and how questions with file and line, so ask it first (ask, to `reference`) and read what it points you to.",
                doing(r),
                holds(def)
            ),
        })
        .collect()
}

fn session_of(def: &FleetDefinition, task: &str) -> Option<Uuid> {
    def.tasks.get(task)?.session.as_deref()?.trim().parse().ok()
}

pub fn on_a_fail(def: &FleetDefinition, status: &FleetStatus) -> Vec<Notice> {
    let Some(asked) = reference(def) else {
        return Vec::new();
    };
    let recent = status.now - Duration::minutes(RECENT_MINUTES);
    status
        .verdicts
        .iter()
        .filter(|v| v.verdict == FAIL && v.at >= recent)
        .flat_map(|v| v.tasks.iter().map(move |task| (v, task)))
        .filter_map(|(verdict, task)| {
            Some(Notice {
                to: session_of(def, task)?,
                key: format!("ask-reference-fail:{}:{task}", verdict.at.timestamp_micros()),
                covers: Vec::new(),
                body: format!(
                    "[mike reference] Acceptance failed your slice of {task} at {}: mike has sent the failure to the reference seat (session {asked}), which holds {}; ask it where and how (ask, to `reference`) before you search, and read its answer first.",
                    verdict.at.format("%H:%M:%SZ"),
                    holds(def)
                ),
            })
        })
        .collect()
}

pub fn on_no_commit(
    def: &FleetDefinition,
    status: &FleetStatus,
    committed: &BTreeSet<String>,
) -> Vec<Notice> {
    let Some(asked) = reference(def) else {
        return Vec::new();
    };
    quiet_seats(def, status, committed)
        .into_iter()
        .filter_map(|quiet| {
            Some(Notice {
                to: quiet.session.trim().parse().ok()?,
                key: format!("ask-reference-quiet:{}", quiet.seat),
                covers: Vec::new(),
                body: format!(
                    "[mike reference] You have worked {} minutes on {} without a commit: the reference seat (session {asked}) holds {}, so ask it the question you are working on (ask, to `reference`) before you search further.",
                    quiet.minutes,
                    quiet.task,
                    holds(def)
                ),
            })
        })
        .collect()
}

pub async fn recent_reads(
    persist: &PersistHandle,
    def: &FleetDefinition,
    now: DateTime<Utc>,
    said: impl Fn(&str) -> bool,
) -> Vec<Reading> {
    let unasked: Vec<Uuid> = seated(def)
        .into_iter()
        .filter(|(_, _, seat, _)| !said(&read_key(seat)))
        .map(|(_, _, _, session)| session)
        .collect();
    if reference(def).is_none() || unasked.is_empty() {
        return Vec::new();
    }
    let tools: Vec<String> = READING_TOOLS.iter().map(|t| (*t).to_owned()).collect();
    let since = now - Duration::minutes(LOOK_BACK_MINUTES);
    match persist
        .session_tool_reads(&unasked, &tools, since, USES_READ)
        .await
    {
        Ok(rows) => {
            let uses: Vec<ToolRead> = rows
                .into_iter()
                .map(|row| ToolRead {
                    session: row.session_uuid.to_string(),
                    tool: row.tool_name,
                    input: row.input,
                })
                .collect();
            readings(def, &uses)
        }
        Err(err) => {
            tracing::warn!(error = %err, "fleet: could not read the seats' reads for the reference seat");
            Vec::new()
        }
    }
}

#[cfg(test)]
#[path = "tell_ask_reference_tests.rs"]
mod tell_ask_reference_tests;
