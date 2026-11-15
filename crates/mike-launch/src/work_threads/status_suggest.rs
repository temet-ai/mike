use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::base_state::BaseState;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, noul};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::tail_chars;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::work_threads::{EntryInput, HistoryPage, ThreadDetail, ThreadState};

pub const POINT: &str = "thread_status";
pub const QUESTION_SET: &str = "thread_status_v1";
pub const SUGGEST_AT: f64 = 0.8;
const FINISHED: &str = "finished";
const PARKED: &str = "parked";
const OPEN_STATUSES: [&str; 3] = ["active", "implementing", "review"];
const FIELD_CHARS: usize = 1200;
const RECENT_ENTRIES: i64 = 5;
const HOLD_HOURS: i64 = 48;
const WAIT_WORDS: [&str; 7] = [
    "waiting",
    "waits",
    "awaiting",
    "pending",
    "blocked",
    "requested",
    "until",
];
const OUTSIDE_STEP_PREFIXES: [&str; 3] = ["deploy", "review", "merge"];
const OUTSIDE_STEP_WORDS: [&str; 4] = ["check", "checks", "ci", "pr"];

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub status: &'static str,
    pub probability: f64,
}

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([
        (
            FINISHED.to_owned(),
            noul(
                "A work thread records the latest state of some work. Does the latest state show the work finished?",
                "Everything the goal asked for is done, and there are no next steps, blockers or open questions left.",
                "Work remains: there are next steps, blockers, open questions or the goal is not yet met.",
            ),
        ),
        (
            PARKED.to_owned(),
            noul(
                "A work thread records the latest state of some work. Does the latest state show the work parked?",
                "The work stopped on purpose and waits on something outside this thread, such as another thread, a decision or a later date.",
                "The work is being carried on, or is finished, or stopped for no stated reason.",
            ),
        ),
    ])
}

fn hours_since_checkpoint(latest: &ThreadState, now: DateTime<Utc>) -> i64 {
    (now - latest.updated_at).num_hours().max(0)
}

fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn names_a_wait(words: &[String]) -> bool {
    words.iter().any(|word| WAIT_WORDS.contains(&word.as_str()))
}

fn names_an_outside_step(words: &[String]) -> bool {
    words.iter().any(|word| {
        OUTSIDE_STEP_PREFIXES
            .iter()
            .any(|prefix| word.starts_with(prefix))
            || OUTSIDE_STEP_WORDS.contains(&word.as_str())
    })
}

fn newest_entry_text(detail: &ThreadDetail) -> String {
    detail
        .history
        .iter()
        .filter(|entry| entry.kind != "finding")
        .max_by_key(|entry| entry.ord)
        .map(|entry| format!("{} {}", entry.summary, entry.body))
        .unwrap_or_default()
}

pub fn waits_on_outside_step(detail: &ThreadDetail, now: DateTime<Utc>) -> bool {
    let Some(latest) = detail.state.as_ref() else {
        return false;
    };
    if hours_since_checkpoint(latest, now) >= HOLD_HOURS {
        return false;
    }
    let text = format!("{} {}", latest.next_up, newest_entry_text(detail));
    let words = words_of(&text);
    names_a_wait(&words) && names_an_outside_step(&words)
}

pub fn state(detail: &ThreadDetail, now: DateTime<Utc>) -> Option<Value> {
    let latest = detail.state.as_ref()?;
    Some(json!({
        "slug": detail.thread.slug,
        "title": detail.thread.title,
        "goal": detail.thread.goal.as_deref().map(|g| tail_chars(g, FIELD_CHARS)),
        "current_status": detail.thread.status,
        "hours_since_last_checkpoint": hours_since_checkpoint(latest, now),
        "tldr": tail_chars(&latest.tldr, FIELD_CHARS),
        "where_we_are": tail_chars(&latest.where_we_are, FIELD_CHARS),
        "next_up": tail_chars(&latest.next_up, FIELD_CHARS),
        "blockers": tail_chars(&latest.blockers, FIELD_CHARS),
        "open_questions": tail_chars(&latest.open_questions, FIELD_CHARS),
    }))
}

pub fn compose(asked: &Asked) -> Option<Suggestion> {
    if asked.failed() {
        return None;
    }
    let finished = asked.noul(FINISHED)?;
    let parked = asked.noul(PARKED)?;
    let (status, probability) = if finished >= parked {
        ("done", finished)
    } else {
        ("parked", parked)
    };
    (probability >= SUGGEST_AT).then_some(Suggestion {
        status,
        probability,
    })
}

fn is_open(detail: &ThreadDetail) -> bool {
    detail
        .thread
        .status
        .as_deref()
        .is_none_or(|status| OPEN_STATUSES.contains(&status))
}

fn already_suggested(detail: &ThreadDetail) -> bool {
    let Some(latest) = detail.state.as_ref() else {
        return false;
    };
    detail.history.iter().any(|entry| {
        entry.kind == "finding"
            && entry.created_at >= latest.updated_at
            && entry
                .data
                .as_ref()
                .and_then(|data| data.get("point"))
                .and_then(Value::as_str)
                == Some(POINT)
    })
}

fn body_of(detail: &ThreadDetail, suggestion: &Suggestion) -> String {
    let current = detail.thread.status.as_deref().unwrap_or("not set");
    let looks = if suggestion.status == "done" {
        "finished"
    } else {
        "parked"
    };
    format!(
        "Jev suggests this thread looks {looks} ({:.0}% sure) from its latest state. The status is still `{current}`; mike does not change it. If the suggestion is right, set the status to `{}` with a checkpoint or a patch.",
        suggestion.probability * 100.0,
        suggestion.status
    )
}

async fn write_finding(
    persist: &PersistHandle,
    project: i64,
    detail: &ThreadDetail,
    suggestion: &Suggestion,
) -> bool {
    let data = json!({
        "point": POINT,
        "suggested_status": suggestion.status,
        "probability": suggestion.probability,
        "current_status": detail.thread.status,
    });
    let summary = format!("Jev suggests status `{}`", suggestion.status);
    let input = EntryInput {
        slug: &detail.thread.slug,
        kind: "finding",
        body: &body_of(detail, suggestion),
        summary: Some(&summary),
        session_uuid: None,
        commit_sha: None,
        data: Some(&data),
    };
    match persist
        .launch()
        .add_work_thread_entry(project, &input)
        .await
    {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!(%error, slug = %detail.thread.slug, "thread_status: could not write the suggestion");
            false
        }
    }
}

pub async fn run(decider: &Decider, persist: &PersistHandle, project: i64, slug: &str) {
    let page = HistoryPage {
        limit: RECENT_ENTRIES,
        before_ord: None,
    };
    let Ok(detail) = persist.launch().get_work_thread(project, slug, page).await else {
        return;
    };
    let now = Utc::now();
    let Some(facts) = state(&detail, now) else {
        return;
    };
    if !is_open(&detail) || already_suggested(&detail) {
        return;
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            facts,
            questions(),
            DecisionSubject {
                project_id: Some(project),
                ..DecisionSubject::default()
            },
            json!({"suggest_at": SUGGEST_AT}),
        )
        .await;
    let waiting = waits_on_outside_step(&detail, now);
    let suggestion = compose(&asked).filter(|_| decider.applies() && !waiting);
    let written = match &suggestion {
        Some(found) => write_finding(persist, project, &detail, found).await,
        None => false,
    };
    let score = suggestion.as_ref().map_or(1.0, |found| found.probability);
    let applied = if written { "suggested" } else { "none" };
    decider
        .mark(
            &asked,
            asked.composed(json!({"score": score, "slug": detail.thread.slug})),
            applied,
        )
        .await;
}

pub fn spawn(state: &BaseState, project: i64, slug: String) {
    let Some(decider) = state.decision_env().and_then(|env| env.decider(POINT)) else {
        return;
    };
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    state.tracker.spawn(async move {
        run(&decider, &persist, project, &slug).await;
    });
}

#[cfg(test)]
#[path = "status_suggest_tests.rs"]
mod tests;
