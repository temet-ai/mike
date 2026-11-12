use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::check_run;
use super::definition::{DoneWhen, FleetDefinition};
use super::events::{EventPayload, FloorFailed, FloorPassed, NewFleetEvent};
use super::ruling::{self, Ruling};
use super::watch::git;
use crate::store::persistence::PersistHandle;

const OUTPUT_LINES: usize = 12;
const MAX_EXIT: i32 = 125;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailCode {
    Check,
    FilesChanged,
    TestsAdded,
    Exercise,
    Unrunnable,
    Definition,
}

impl FailCode {
    pub fn as_str(self) -> &'static str {
        match self {
            FailCode::Check => "check_failed",
            FailCode::FilesChanged => "files_not_changed",
            FailCode::TestsAdded => "no_test_added",
            FailCode::Exercise => "exercise_failed",
            FailCode::Unrunnable => "floor_unrunnable",
            FailCode::Definition => "floor_definition_problem",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed {
        sha: String,
    },
    Failed {
        sha: String,
        code: FailCode,
        line: String,
    },
}

impl Outcome {
    pub fn reaches_judge(&self) -> bool {
        matches!(self, Outcome::Passed { .. })
    }

    pub fn sha(&self) -> &str {
        match self {
            Outcome::Passed { sha } | Outcome::Failed { sha, .. } => sha,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub operation: String,
    pub command: String,
    pub exit: Option<i32>,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub checks: Vec<Observed>,
    pub touched: Vec<String>,
    pub added: Vec<String>,
    pub exercise: Option<Observed>,
}

fn tail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let from = lines.len().saturating_sub(OUTPUT_LINES);
    lines[from..].join("\n")
}

fn under(path: &str, prefix: &str) -> bool {
    let prefix = prefix.trim().trim_end_matches('/');
    !prefix.is_empty()
        && (path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/')))
}

fn failed(sha: &str, code: FailCode, line: String) -> Outcome {
    Outcome::Failed {
        sha: sha.to_string(),
        code,
        line,
    }
}

pub fn evaluate(done: &DoneWhen, sha: &str, evidence: &Evidence) -> Outcome {
    for (check, seen) in done.checks.iter().zip(&evidence.checks) {
        if seen.exit != Some(0)
            && super::nextest_filter::matched_no_test(&seen.command, seen.exit, &seen.output)
        {
            let item = format!("the done_when check `{}`", check.operation);
            return failed(
                sha,
                FailCode::Definition,
                super::nextest_filter::definition_problem(&item),
            );
        }
        if seen.exit != Some(0) {
            let exit = seen
                .exit
                .map_or("none".to_string(), |code| code.to_string());
            return failed(
                sha,
                FailCode::Check,
                format!(
                    "check `{}` did not meet exit {} (observed {exit}):\n{}",
                    check.operation,
                    check.expected_exit,
                    tail(&seen.output)
                ),
            );
        }
    }
    for wanted in &done.files_changed {
        if !evidence.touched.iter().any(|path| under(path, wanted)) {
            return failed(
                sha,
                FailCode::FilesChanged,
                format!("`{wanted}` is not changed by this task's commit or tree changes"),
            );
        }
    }
    for wanted in &done.tests_added_under {
        if !evidence.added.iter().any(|path| under(path, wanted)) {
            return failed(
                sha,
                FailCode::TestsAdded,
                format!("no file was added under `{wanted}`; add a test there"),
            );
        }
    }
    if let (Some(exercise), Some(seen)) = (&done.exercise, &evidence.exercise) {
        if seen.exit != Some(0) {
            return failed(
                sha,
                FailCode::Exercise,
                format!(
                    "exercise `{}` did not exit 0 (observed {:?}):\n{}",
                    exercise.command,
                    seen.exit,
                    tail(&seen.output)
                ),
            );
        }
        if !exercise.expect_output.is_empty() && !seen.output.contains(&exercise.expect_output) {
            return failed(
                sha,
                FailCode::Exercise,
                format!(
                    "exercise `{}` output lacks `{}`:\n{}",
                    exercise.command,
                    exercise.expect_output,
                    tail(&seen.output)
                ),
            );
        }
    }
    Outcome::Passed {
        sha: sha.to_string(),
    }
}

pub fn event(run: &str, task: &str, seat: Uuid, outcome: &Outcome) -> NewFleetEvent {
    let payload = match outcome {
        Outcome::Passed { sha } => EventPayload::FloorPassed(FloorPassed { sha: sha.clone() }),
        Outcome::Failed { sha, line, .. } => EventPayload::FloorFailed(FloorFailed {
            sha: sha.clone(),
            check: line.clone(),
        }),
    };
    NewFleetEvent {
        run_id: run.to_string(),
        task: Some(task.to_string()),
        session_uuid: Some(seat),
        payload,
    }
}

pub fn seat_answer(task: &str, outcome: &Outcome) -> Option<String> {
    match outcome {
        Outcome::Passed { .. } => None,
        Outcome::Failed {
            sha,
            code: FailCode::Definition,
            line,
        } => Some(format!(
            "[mike floor] Task {task}'s declared floor could not judge {sha}: {line}\nYour work is not failed. Tell the coordinator so it fixes tasks.{task}.done_when, then report again with fleet_report."
        )),
        Outcome::Failed { sha, code, line } => Some(format!(
            "[mike floor] Task {task} failed its declared floor at {sha} ({}): {line}\nFix it, commit, and report again with fleet_report. No judge has read this report.",
            code.as_str()
        )),
    }
}

fn ruled(def: &FleetDefinition, command: &str, expected: i32) -> FleetDefinition {
    ruling::definition(
        def,
        &Ruling {
            version: 1,
            command: command.to_string(),
            expected_exit_code: expected,
        },
    )
}

async fn observe(
    (def, run): (&FleetDefinition, &str),
    sha: &str,
    (name, command, expected): (&str, &str, i32),
) -> Observed {
    let ledger = format!("{run}#ruling:floor-{name}#baseline:{sha}");
    let def = ruled(def, command, expected);
    let (exit, output) = match check_run::target(&def, Some(sha)).await {
        Ok(target) => check_run::execute(&def, &target, &ledger).await,
        Err(why) => (None, why),
    };
    Observed {
        operation: name.to_string(),
        command: command.to_string(),
        exit,
        output,
    }
}

fn unrunnable(def: &FleetDefinition, done: &DoneWhen) -> Option<String> {
    for check in &done.checks {
        if !def.execution.operations.contains_key(&check.operation) {
            return Some(format!(
                "done_when names operation `{}` that this fleet does not declare",
                check.operation
            ));
        }
        if !(0..=MAX_EXIT).contains(&check.expected_exit) {
            return Some(format!(
                "done_when check `{}` expects exit {}; it must be 0 to {MAX_EXIT}",
                check.operation, check.expected_exit
            ));
        }
    }
    None
}

fn parse_name_status(text: &str) -> (Vec<String>, Vec<String>) {
    let mut touched = Vec::new();
    let mut added = Vec::new();
    for line in text.lines() {
        let Some((status, path)) = line.split_once('\t') else {
            continue;
        };
        touched.push(path.to_string());
        if status.starts_with('A') {
            added.push(path.to_string());
        }
    }
    (touched, added)
}

async fn commit_files(repo: &str, sha: &str) -> (Vec<String>, Vec<String>) {
    let args = ["show", "--name-status", "--no-renames", "--format=", sha];
    let raw = git(repo, &args, None).await.unwrap_or_default();
    parse_name_status(&String::from_utf8_lossy(&raw))
}

const FULL_SUITE_COMMANDS: [&str; 6] = [
    "scripts/gate",
    "make check",
    "make test-all",
    "make test-db",
    "make gate",
    "make lint",
];
const TEST_RUNNERS: [&str; 2] = ["nextest run", "cargo test"];
const VALUE_FLAGS: [&str; 14] = [
    "-p",
    "--package",
    "--profile",
    "--features",
    "-F",
    "--target-dir",
    "--test",
    "--bin",
    "--example",
    "--bench",
    "-j",
    "--jobs",
    "--manifest-path",
    "--config",
];
const FILTER_FLAGS: [&str; 4] = ["-E", "--filterset", "--filter-expr", "--"];

fn after_runner(command: &str) -> Option<&str> {
    TEST_RUNNERS
        .iter()
        .find_map(|runner| command.find(runner).map(|at| &command[at + runner.len()..]))
}

fn names_a_filter(arguments: &str) -> bool {
    let mut words = arguments.split_whitespace();
    while let Some(word) = words.next() {
        if matches!(word, "&&" | ";" | "|" | "||") {
            return false;
        }
        if FILTER_FLAGS.contains(&word) {
            return true;
        }
        if VALUE_FLAGS.contains(&word) {
            words.next();
        } else if !word.starts_with('-') {
            return true;
        }
    }
    false
}

pub fn heavy_reason(command: &str) -> Option<&'static str> {
    let command = command.trim();
    if command.contains("--workspace") || command.contains("--all-targets") {
        return Some("it builds or tests the whole workspace");
    }
    if FULL_SUITE_COMMANDS
        .iter()
        .any(|full| command.contains(full))
    {
        return Some("it runs the project's full gate or test suite");
    }
    if command.contains("make test-p")
        || (command.contains("make test") && !command.contains("make test-t"))
    {
        return Some("it runs every test of a crate");
    }
    let arguments = after_runner(command)?;
    (!names_a_filter(arguments)).then_some("it runs every test of a crate, with no test filter")
}

fn floor_commands(def: &FleetDefinition, done: &DoneWhen) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = done
        .checks
        .iter()
        .filter_map(|check| {
            let op = def.execution.operations.get(&check.operation)?;
            Some((
                format!("done_when check `{}`", check.operation),
                op.command.clone(),
            ))
        })
        .collect();
    found.extend(
        done.exercise
            .iter()
            .map(|exercise| ("done_when exercise".to_string(), exercise.command.clone())),
    );
    found
}

pub fn heavy_floor_warnings(def: &FleetDefinition) -> Vec<super::Missing> {
    let mut warnings = Vec::new();
    for (id, task) in &def.tasks {
        let Some(done) = &task.done_when else {
            continue;
        };
        for (what, command) in floor_commands(def, done) {
            if let Some(why) = heavy_reason(&command) {
                warnings.push(super::Missing::new(
                    format!("tasks.{id}.done_when"),
                    format!(
                        "the {what} runs `{}`, and {why}; mike runs a floor at every report of the task, in the check slot, so a whole-crate or full-suite floor holds every report for many minutes. Name the task's own tests (a nextest filter such as `-p mike --lib launch::fleet::floor`) and leave the full suite to gate.full_command",
                        command.trim()
                    ),
                ));
            }
        }
    }
    warnings
}

pub struct FloorInput<'a> {
    pub def: &'a FleetDefinition,
    pub run: &'a str,
    pub task: &'a str,
    pub seat: Uuid,
    pub sha: &'a str,
    pub since: DateTime<Utc>,
}

async fn gather(persist: &PersistHandle, input: &FloorInput<'_>, done: &DoneWhen) -> Evidence {
    let scope = (input.def, input.run);
    let mut checks = Vec::new();
    for check in &done.checks {
        let command = input
            .def
            .execution
            .operations
            .get(&check.operation)
            .map(|op| op.command.as_str())
            .unwrap_or_default();
        let spec = (check.operation.as_str(), command, check.expected_exit);
        checks.push(observe(scope, input.sha, spec).await);
    }
    let exercise = match &done.exercise {
        Some(exercise) => {
            let spec = ("exercise", exercise.command.as_str(), 0);
            Some(observe(scope, input.sha, spec).await)
        }
        None => None,
    };
    let (mut touched, added) = commit_files(&input.def.repo, input.sha).await;
    let since = input.since - Duration::microseconds(1);
    if let Ok(rows) = persist.tree_changed_paths_since(input.run, since).await {
        touched.extend(rows.into_iter().map(|row| row.path));
    }
    Evidence {
        checks,
        touched,
        added,
        exercise,
    }
}

async fn record(persist: &PersistHandle, input: &FloorInput<'_>, outcome: &Outcome) {
    let new = event(input.run, input.task, input.seat, outcome);
    if let Err(err) = persist.append_fleet_event(&new).await {
        tracing::warn!(error = %err, task = input.task, "fleet floor: could not record the floor event");
    }
}

async fn answer(persist: &PersistHandle, input: &FloorInput<'_>, outcome: &Outcome) {
    let Some(text) = seat_answer(input.task, outcome) else {
        return;
    };
    if let Err(err) = persist
        .insert_message(Uuid::new_v4(), input.seat, None, &text)
        .await
    {
        tracing::warn!(error = %err, task = input.task, "fleet floor: could not answer the seat");
    }
}

async fn decide(persist: &PersistHandle, input: &FloorInput<'_>, done: &DoneWhen) -> Outcome {
    match unrunnable(input.def, done) {
        Some(line) => failed(input.sha, FailCode::Unrunnable, line),
        None => evaluate(done, input.sha, &gather(persist, input, done).await),
    }
}

pub async fn run(persist: &PersistHandle, input: &FloorInput<'_>) -> Option<Outcome> {
    let done = input.def.tasks.get(input.task)?.done_when.as_ref()?;
    let outcome = decide(persist, input, done).await;
    record(persist, input, &outcome).await;
    answer(persist, input, &outcome).await;
    Some(outcome)
}

#[cfg(test)]
#[path = "floor_tests.rs"]
mod tests;
