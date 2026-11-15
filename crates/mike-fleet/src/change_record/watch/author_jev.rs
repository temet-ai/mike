use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::author::{Authorship, Window, written_texts};
use crate::change_record::ShellRun;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, choice, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::change_record::EditCall;

pub const POINT: &str = "change_author";
pub const QUESTION_SET: &str = "change_author_v4";
const WHO: &str = "who";
const WRITES: &str = "writes";
const UNKNOWN: &str = "unknown";
const WRITES_AT: f64 = 0.7;
const PICK_AT: f64 = 0.6;
const COMMAND_CHARS: usize = 300;

pub struct Facts<'a> {
    pub path: &'a str,
    pub calls: &'a [EditCall],
    pub runs: &'a [ShellRun],
    pub after: Option<&'a str>,
    pub changed: Window,
}

fn key(session: i64) -> String {
    format!("s{session}")
}

fn wrote_into_file(call: &EditCall, after: Option<&str>) -> Option<bool> {
    let after = after?;
    let texts = written_texts(call)?;
    Some(texts.iter().all(|text| after.contains(text)))
}

fn overlaps(started: DateTime<Utc>, ended: DateTime<Utc>, changed: Window) -> bool {
    started <= changed.to && ended >= changed.from
}

fn timing(started: DateTime<Utc>, ended: Option<DateTime<Utc>>, changed: Window) -> Value {
    let ended = ended.unwrap_or(started);
    json!({
        "overlaps_change_window": if overlaps(started, ended, changed) { "yes" } else { "no" },
        "ended_seconds_after_change": (ended - changed.to).num_seconds(),
    })
}

fn merged(mut base: Value, extra: Value) -> Value {
    if let (Some(into), Value::Object(from)) = (base.as_object_mut(), extra) {
        into.extend(from);
    }
    base
}

pub fn trim_command(command: &str, path: &str) -> String {
    if command.chars().count() <= COMMAND_CHARS {
        return command.to_owned();
    }
    let named: Vec<&str> = command
        .lines()
        .filter(|line| !path.is_empty() && line.contains(path))
        .collect();
    let kept = if named.is_empty() {
        command.to_owned()
    } else {
        named.join("\n")
    };
    let before = kept
        .find(path)
        .filter(|_| !named.is_empty())
        .map_or(0, |at| kept[..at].chars().count());
    kept.chars()
        .skip(before.saturating_sub(COMMAND_CHARS / 3))
        .take(COMMAND_CHARS)
        .collect()
}

fn edits_of(session: i64, facts: &Facts<'_>) -> Vec<Value> {
    facts
        .calls
        .iter()
        .filter(|call| call.session_id == session)
        .map(|call| {
            merged(
                json!({
                    "tool": call.tool_name,
                    "failed": call.failed,
                    "its_text_is_in_the_final_file": wrote_into_file(call, facts.after),
                }),
                timing(call.ran_from, call.ran_to, facts.changed),
            )
        })
        .collect()
}

fn touches(run: &ShellRun, path: &str) -> bool {
    run.names_checkout
        || run
            .command
            .as_deref()
            .is_some_and(|command| command.contains(path))
}

pub fn has_evidence(candidates: &[i64], facts: &Facts<'_>) -> bool {
    facts
        .calls
        .iter()
        .any(|call| candidates.contains(&call.session_id))
        || facts
            .runs
            .iter()
            .any(|run| candidates.contains(&run.session_id) && touches(run, facts.path))
}

fn shells_of(session: i64, facts: &Facts<'_>) -> Vec<Value> {
    facts
        .runs
        .iter()
        .filter(|run| run.session_id == session)
        .map(|run| {
            merged(
                json!({
                    "command": run.command.as_deref().map(|c| trim_command(c, facts.path)),
                }),
                timing(run.started, run.finished, facts.changed),
            )
        })
        .collect()
}

pub fn candidate_facts(session: i64, facts: &Facts<'_>) -> Value {
    json!({
        "id": key(session),
        "edit_tool_calls_naming_the_file": edits_of(session, facts),
        "shell_commands_running_in_the_checkout": shells_of(session, facts),
    })
}

pub fn state(candidates: &[i64], facts: &Facts<'_>) -> Value {
    json!({
        "changed_file": facts.path,
        "candidate_sessions": candidates
            .iter()
            .map(|session| candidate_facts(*session, facts))
            .collect::<Vec<_>>(),
    })
}

pub fn questions(candidates: &[i64]) -> BTreeMap<String, Question> {
    let mut options: Vec<(String, String)> = candidates
        .iter()
        .map(|session| {
            (
                key(*session),
                format!(
                    "Session {session}: a command or edit tool call that overlaps the change window writes `changed_file`."
                ),
            )
        })
        .collect();
    options.push((
        UNKNOWN.to_owned(),
        "The facts do not show which of these sessions made the change.".to_owned(),
    ));
    let listed: Vec<(&str, &str)> = options
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    BTreeMap::from([
        (
            WHO.to_owned(),
            choice(
                "Several sessions worked in this checkout while `changed_file` changed. Read each candidate's calls where `overlaps_change_window` is yes. Which session made the change?",
                &listed,
            ),
        ),
        (
            WRITES.to_owned(),
            noul(
                "Does any candidate's command or edit that overlaps the change window write `changed_file`?",
                "A call with overlaps_change_window yes writes changed_file",
                "No call with overlaps_change_window yes writes changed_file",
            ),
        ),
    ])
}

pub fn compose(asked: &Asked, candidates: &[i64]) -> Option<(i64, f64)> {
    if asked.failed() || asked.noul(WRITES)? < WRITES_AT {
        return None;
    }
    let (picked, probability) = asked.choice(WHO)?;
    if probability < PICK_AT {
        return None;
    }
    candidates
        .iter()
        .find(|session| key(**session) == picked)
        .map(|session| (*session, probability))
}

pub async fn resolve(
    decider: &Decider,
    candidates: &[i64],
    facts: &Facts<'_>,
) -> Option<Authorship> {
    let asked = decider
        .ask(
            QUESTION_SET,
            state(candidates, facts),
            questions(candidates),
            DecisionSubject::default(),
            json!({"writes_at": WRITES_AT, "pick_at": PICK_AT}),
        )
        .await;
    let composed = compose(&asked, candidates);
    let score = composed.map_or(1.0, |(_, p)| p);
    let applied = if composed.is_some() && decider.applies() {
        "chosen"
    } else {
        "none"
    };
    decider
        .mark(&asked, asked.composed(json!({"score": score})), applied)
        .await;
    composed
        .filter(|_| decider.applies())
        .map(|(session, probability)| Authorship::Judged {
            session,
            percent: (probability * 100.0).round() as u32,
        })
}

#[cfg(test)]
#[path = "author_jev_tests.rs"]
mod tests;
