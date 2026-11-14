use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::author::{Authorship, written_texts};
use crate::change_record::ShellRun;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::change_record::EditCall;

pub const POINT: &str = "change_author";
pub const QUESTION_SET: &str = "change_author_v1";
const WHO: &str = "who";
const UNKNOWN: &str = "unknown";

pub struct Facts<'a> {
    pub path: &'a str,
    pub calls: &'a [EditCall],
    pub runs: &'a [ShellRun],
    pub after: Option<&'a str>,
}

fn key(session: i64) -> String {
    format!("s{session}")
}

fn wrote_into_file(call: &EditCall, after: Option<&str>) -> Option<bool> {
    let after = after?;
    let texts = written_texts(call)?;
    Some(texts.iter().all(|text| after.contains(text)))
}

fn edits_of(session: i64, facts: &Facts<'_>) -> Vec<Value> {
    facts
        .calls
        .iter()
        .filter(|call| call.session_id == session)
        .map(|call| {
            json!({
                "tool": call.tool_name,
                "at": call.at,
                "failed": call.failed,
                "its_text_is_in_the_final_file": wrote_into_file(call, facts.after),
            })
        })
        .collect()
}

fn shells_of(session: i64, facts: &Facts<'_>) -> Vec<Value> {
    facts
        .runs
        .iter()
        .filter(|run| run.session_id == session)
        .map(|run| json!({"started": run.started, "finished": run.finished}))
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
                    "Session {session} made this change, as its edit tool calls or shell commands show."
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
    BTreeMap::from([(
        WHO.to_owned(),
        choice(
            "Several sessions worked in this checkout while a file changed. Which session made the change to the file?",
            &listed,
        ),
    )])
}

pub fn compose(asked: &Asked, candidates: &[i64]) -> Option<(i64, f64)> {
    if asked.failed() {
        return None;
    }
    let (picked, probability) = asked.choice(WHO)?;
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
            json!({}),
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
