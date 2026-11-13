use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;

use super::checks::Missing;
use super::definition::FleetDefinition;

pub const RAN_NO_TEST: &str = "ran no test";
const NEXTEST_RAN_NOTHING: &str = "no tests to run";
const NEXTEST_NO_TESTS_EXIT: i32 = 4;
const RULED_NO_TESTS_EXIT: &str = "[mike ruling] observed exit: 4;";
const LIST_PATIENCE: Duration = Duration::from_secs(900);
const SHELL_JOINERS: [&str; 6] = ["|", ";", "&&", "||", ">", "<"];

pub fn is_nextest_run(command: &str) -> bool {
    let words: Vec<&str> = command
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .collect();
    words
        .windows(2)
        .any(|pair| pair[0] == "nextest" && pair[1] == "run")
}

pub fn matched_no_test(command: &str, exit: Option<i32>, output: &str) -> bool {
    is_nextest_run(command)
        && output.contains(NEXTEST_RAN_NOTHING)
        && (exit == Some(NEXTEST_NO_TESTS_EXIT) || output.contains(RULED_NO_TESTS_EXIT))
}

pub fn definition_problem(item: &str) -> String {
    format!(
        "the filter matched no test under this profile (nextest exit 4, no tests run), so nothing was tested; this is a problem in {item}, not a failure of the task. Fix the filter, or name the profile that runs those tests (for example `--profile ci` for DB-tier tests)"
    )
}

pub fn list_command(command: &str) -> Option<String> {
    if !is_nextest_run(command)
        || SHELL_JOINERS
            .iter()
            .any(|joiner| command.split_whitespace().any(|word| word.contains(joiner)))
    {
        return None;
    }
    let listed = command.replacen("nextest run", "nextest list", 1);
    let format = " --message-format json";
    Some(match listed.split_once(" -- ") {
        Some((before, after)) => format!("{before}{format} -- {after}"),
        None => format!("{listed}{format}"),
    })
}

fn matching() -> Option<&'static Regex> {
    static MATCHES: OnceLock<Option<Regex>> = OnceLock::new();
    MATCHES
        .get_or_init(|| Regex::new(r#""status"\s*:\s*"matches""#).ok())
        .as_ref()
}

pub fn matched(listing: &str) -> Option<usize> {
    if !listing.contains("\"rust-suites\"") {
        return None;
    }
    matching().map(|pattern| pattern.find_iter(listing).count())
}

async fn listed(repo: &str, command: &str) -> Result<String, String> {
    let mut process = tokio::process::Command::new("sh");
    process
        .arg("-c")
        .arg(command)
        .current_dir(repo)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(LIST_PATIENCE, process.output())
        .await
        .map_err(|_| format!("did not finish within {}s", LIST_PATIENCE.as_secs()))?
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let last: Vec<&str> = said.lines().rev().take(6).collect();
    Err(format!(
        "exited {:?}: {}",
        output.status.code(),
        last.into_iter().rev().collect::<Vec<_>>().join("\n")
    ))
}

pub async fn done_when_filters(def: &FleetDefinition) -> Vec<Missing> {
    let mut found = Vec::new();
    for (id, task) in &def.tasks {
        let Some(done) = &task.done_when else {
            continue;
        };
        for (index, check) in done.checks.iter().enumerate() {
            let Some(operation) = def.execution.operations.get(&check.operation) else {
                continue;
            };
            let Some(list) = list_command(&operation.command) else {
                continue;
            };
            let item = format!("tasks.{id}.done_when.checks.{index}");
            match listed(&def.repo, &list).await.map(|out| matched(&out)) {
                Ok(Some(0)) => found.push(Missing::new(&item, definition_problem(&item))),
                Ok(_) => {}
                Err(why) => found.push(Missing::new(
                    &item,
                    format!("`{list}` could not confirm the filter matches a test: {why}"),
                )),
            }
        }
    }
    found
}

#[cfg(test)]
#[path = "nextest_filter_tests.rs"]
pub mod tests;
