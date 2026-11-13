use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::author::{Author, author_of};
use super::error::WriterError;
use super::paths::{commit_paths, paths_named};
use crate::change_record::{ChangeKind, NewChangeEvent};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::mailbox::ContextItem;

#[derive(Debug, Clone)]
pub struct NoteFacts {
    pub note_uuid: Uuid,
    pub note_kind: String,
    pub title: String,
    pub body: String,
    pub status: Option<String>,
    pub data: Value,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MailFacts {
    pub message_uuid: Uuid,
    pub from: Option<Uuid>,
    pub to: Uuid,
    pub body: String,
    pub context: Vec<ContextItem>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CheckFacts {
    pub project_id: i64,
    pub checkout: String,
    pub run_id: Option<String>,
    pub label: String,
    pub command: String,
    pub commit: String,
    pub passed: bool,
    pub source_verified: bool,
    pub failing_paths: Vec<String>,
    pub at: DateTime<Utc>,
}

fn relative_to(checkout: &str, path: &str) -> String {
    let prefix = format!("{}/", checkout.trim_end_matches('/'));
    path.strip_prefix(&prefix).unwrap_or(path).to_string()
}

fn push_new(into: &mut Vec<String>, path: String) {
    if !path.is_empty() && !path.contains('\u{1f}') && !into.contains(&path) {
        into.push(path);
    }
}

fn data_paths(data: &Value, checkout: &str) -> Vec<String> {
    let mut found = Vec::new();
    if let Some(listed) = data.get("paths").and_then(Value::as_array) {
        for path in listed.iter().filter_map(Value::as_str) {
            push_new(&mut found, relative_to(checkout, path));
        }
    }
    found
}

async fn derived_paths(text: &str, checkout: &str) -> Vec<String> {
    match paths_named(text, Path::new(checkout)).await {
        Ok(found) => found,
        Err(err) => {
            tracing::warn!(error = %err, checkout, "change record: could not work out paths, recording fewer");
            Vec::new()
        }
    }
}

async fn merged_paths(first: Vec<String>, text: &str, checkout: &str) -> Vec<String> {
    let mut all = first;
    for path in derived_paths(text, checkout).await {
        push_new(&mut all, path);
    }
    all
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn short(commit: &str) -> &str {
    commit.get(..10).unwrap_or(commit)
}

fn note_event(author: &Author, facts: &NoteFacts, paths: Vec<String>) -> NewChangeEvent {
    let has_verdict = facts.data.get("verdict").is_some();
    let mut detail = json!({
        "note_uuid": facts.note_uuid,
        "note_kind": facts.note_kind,
        "status": facts.status,
    });
    if let Some(verdict) = facts.data.get("verdict") {
        detail["verdict"] = verdict.clone();
    }
    NewChangeEvent {
        project_id: author.project_id,
        checkout: author.checkout.clone(),
        at: facts.at,
        kind: if has_verdict {
            ChangeKind::Verdict
        } else {
            ChangeKind::Note
        },
        session_id: Some(author.session_id),
        run_id: author.run_id.clone(),
        task: author.task.clone(),
        paths,
        subject: facts.title.clone(),
        detail,
        before_sha: None,
        after_sha: None,
    }
}

pub async fn record_note(
    persist: &PersistHandle,
    session: Uuid,
    facts: &NoteFacts,
) -> Result<Option<i64>, WriterError> {
    let Some(author) = author_of(persist, session).await? else {
        return Ok(None);
    };
    let first = data_paths(&facts.data, &author.checkout);
    let text = format!("{}\n{}", facts.title, facts.body);
    let paths = merged_paths(first, &text, &author.checkout).await;
    let ids = persist
        .fleet()
        .insert_change_events(&[note_event(&author, facts, paths)])
        .await?;
    Ok(ids.first().copied())
}

async fn context_paths(items: &[ContextItem], checkout: &str) -> Vec<String> {
    let mut found = Vec::new();
    for item in items.iter().filter(|i| i.kind == "file") {
        push_new(&mut found, relative_to(checkout, item.value.trim()));
    }
    for item in items.iter().filter(|i| i.kind == "commit") {
        for path in commit_paths(Path::new(checkout), item.value.trim()).await {
            push_new(&mut found, path);
        }
    }
    found
}

async fn mail_author(
    persist: &PersistHandle,
    facts: &MailFacts,
) -> Result<Option<Author>, WriterError> {
    if let Some(from) = facts.from
        && let Some(author) = author_of(persist, from).await?
    {
        return Ok(Some(author));
    }
    Ok(author_of(persist, facts.to).await?.map(|mut author| {
        author.task = None;
        author.run_id = None;
        author
    }))
}

pub async fn record_mail(
    persist: &PersistHandle,
    facts: &MailFacts,
) -> Result<Option<i64>, WriterError> {
    let Some(author) = mail_author(persist, facts).await? else {
        return Ok(None);
    };
    let from_known = facts.from.is_some();
    let first = context_paths(&facts.context, &author.checkout).await;
    let paths = merged_paths(first, &facts.body, &author.checkout).await;
    let event = NewChangeEvent {
        project_id: author.project_id,
        checkout: author.checkout,
        at: facts.at,
        kind: ChangeKind::Mail,
        session_id: from_known.then_some(author.session_id),
        run_id: author.run_id,
        task: author.task,
        paths,
        subject: first_line(&facts.body),
        detail: json!({ "message_uuid": facts.message_uuid, "to": facts.to }),
        before_sha: None,
        after_sha: None,
    };
    let ids = persist.fleet().insert_change_events(&[event]).await?;
    Ok(ids.first().copied())
}

pub async fn record_check(
    persist: &PersistHandle,
    facts: CheckFacts,
) -> Result<Option<i64>, WriterError> {
    let mut paths = Vec::new();
    for path in &facts.failing_paths {
        push_new(&mut paths, relative_to(&facts.checkout, path));
    }
    let outcome = if facts.passed { "passed" } else { "failed" };
    let event = NewChangeEvent {
        project_id: facts.project_id,
        checkout: facts.checkout,
        at: facts.at,
        kind: ChangeKind::Check,
        session_id: None,
        run_id: facts.run_id,
        task: None,
        paths,
        subject: format!("{} {} at {}", facts.label, outcome, short(&facts.commit)),
        detail: json!({
            "label": facts.label,
            "command": facts.command,
            "commit": facts.commit,
            "passed": facts.passed,
            "source_verified": facts.source_verified,
            "proof_version": 1,
        }),
        before_sha: None,
        after_sha: Some(facts.commit),
    };
    let ids = persist.fleet().insert_change_events(&[event]).await?;
    Ok(ids.first().copied())
}

pub fn spawn_note_event(
    tracker: &tokio_util::task::TaskTracker,
    persist: PersistHandle,
    enabled: bool,
    session: Uuid,
    facts: NoteFacts,
) {
    if !enabled {
        return;
    }
    tracker.spawn(async move {
        if let Err(err) = record_note(&persist, session, &facts).await {
            tracing::warn!(error = %err, note = %facts.note_uuid, "change record: could not record a note");
        }
    });
}

pub fn spawn_mail_event(
    tracker: &tokio_util::task::TaskTracker,
    persist: PersistHandle,
    enabled: bool,
    facts: MailFacts,
) {
    if !enabled {
        return;
    }
    tracker.spawn(async move {
        if let Err(err) = record_mail(&persist, &facts).await {
            tracing::warn!(error = %err, message = %facts.message_uuid, "change record: could not record a mail");
        }
    });
}
