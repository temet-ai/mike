use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use super::git::Git;
use crate::change_record::{ChangeKind, ShellRun, ToolCallNaming};
use crate::store::persistence::change_record::EditCall;
use crate::store::persistence::{PersistError, PersistHandle};

const RUN_SLACK_MS: i64 = 1000;
const LATE_ROW_MS: i64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authorship {
    Edit {
        session: i64,
        tool_use_id: i64,
        tool_name: String,
        content_verified: bool,
    },
    Shell {
        session: i64,
        named_by: Option<i64>,
    },
    Seat {
        session: i64,
    },
    Unknown {
        candidates: Vec<i64>,
    },
}

impl Authorship {
    pub fn session(&self) -> Option<i64> {
        match self {
            Authorship::Edit { session, .. }
            | Authorship::Shell { session, .. }
            | Authorship::Seat { session } => Some(*session),
            Authorship::Unknown { .. } => None,
        }
    }

    pub fn file_kind(&self) -> ChangeKind {
        match self {
            Authorship::Edit { .. } => ChangeKind::Edit,
            _ => ChangeKind::ShellChange,
        }
    }

    pub fn detail(&self) -> serde_json::Value {
        match self {
            Authorship::Edit {
                tool_use_id,
                tool_name,
                content_verified,
                ..
            } => {
                json!({"author": "tool_call", "tool_use_id": tool_use_id, "tool_name": tool_name, "content_verified": content_verified})
            }
            Authorship::Shell {
                named_by: Some(tool_use_id),
                ..
            } => json!({"author": "shell", "named_by_tool_use_id": tool_use_id}),
            Authorship::Shell { named_by: None, .. } => {
                json!({"author": "shell", "inferred": true})
            }
            Authorship::Seat { .. } => json!({"author": "fleet_seat_trailer"}),
            Authorship::Unknown { candidates } => {
                json!({"author": "unknown", "candidates": candidates})
            }
        }
    }
}

pub fn decide(calls: &[ToolCallNaming], runs: &[ShellRun]) -> Authorship {
    let Some(newest) = calls.iter().max_by_key(|c| (c.at, c.tool_use_id)) else {
        return from_shell_runs(runs);
    };
    let sessions: BTreeSet<i64> = calls
        .iter()
        .map(|c| c.session_id)
        .chain(runs.iter().map(|r| r.session_id))
        .collect();
    if sessions.len() > 1 {
        return Authorship::Unknown {
            candidates: sessions.into_iter().collect(),
        };
    }
    Authorship::Edit {
        session: newest.session_id,
        tool_use_id: newest.tool_use_id,
        tool_name: newest.tool_name.clone(),
        content_verified: false,
    }
}

pub fn from_shell_runs(runs: &[ShellRun]) -> Authorship {
    let sessions: BTreeSet<i64> = runs.iter().map(|r| r.session_id).collect();
    let mut each = sessions.iter();
    match (each.next(), each.next()) {
        (Some(one), None) => Authorship::Shell {
            session: *one,
            named_by: None,
        },
        _ => Authorship::Unknown {
            candidates: sessions.into_iter().collect(),
        },
    }
}

pub fn written_texts(call: &EditCall) -> Option<Vec<&str>> {
    let input = call.input.as_ref()?;
    let texts: Vec<&str> = match call.tool_name.as_str() {
        "Edit" => vec![input.get("new_string")?.as_str()?],
        "Write" => vec![input.get("content")?.as_str()?],
        "MultiEdit" => input
            .get("edits")?
            .as_array()?
            .iter()
            .map(|e| e.get("new_string").and_then(serde_json::Value::as_str))
            .collect::<Option<Vec<&str>>>()?,
        _ => return None,
    };
    (!texts.is_empty() && texts.iter().all(|t| !t.is_empty())).then_some(texts)
}

pub fn ran_at(call: &EditCall, at: DateTime<Utc>) -> bool {
    let slack = Duration::milliseconds(RUN_SLACK_MS);
    call.failed != Some(true)
        && at >= call.ran_from - slack
        && call.ran_to.is_none_or(|to| at <= to + slack)
}

fn proven_calls<'a>(calls: &'a [EditCall], at: DateTime<Utc>, after: &str) -> Vec<&'a EditCall> {
    let proven: Vec<&EditCall> = calls
        .iter()
        .filter(|c| ran_at(c, at))
        .filter(|c| written_texts(c).is_some_and(|texts| texts.iter().all(|t| after.contains(t))))
        .collect();
    let sessions: BTreeSet<i64> = proven.iter().map(|c| c.session_id).collect();
    if sessions.len() == 1 {
        proven
    } else {
        Vec::new()
    }
}

fn edit_authorship(proven: &[&EditCall]) -> Option<Authorship> {
    let newest = proven.iter().max_by_key(|c| (c.at, c.tool_use_id))?;
    Some(Authorship::Edit {
        session: newest.session_id,
        tool_use_id: newest.tool_use_id,
        tool_name: newest.tool_name.clone(),
        content_verified: true,
    })
}

pub fn proven_edit(calls: &[EditCall], at: DateTime<Utc>, after: &str) -> Option<Authorship> {
    edit_authorship(&proven_calls(calls, at, after))
}

fn apply_edit(text: String, edit: &serde_json::Value) -> Option<String> {
    let old = edit.get("old_string")?.as_str()?;
    let new = edit.get("new_string")?.as_str()?;
    if old.is_empty() || !text.contains(old) {
        return None;
    }
    let all = edit
        .get("replace_all")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Some(if all {
        text.replace(old, new)
    } else {
        text.replacen(old, new, 1)
    })
}

fn apply_call(text: Option<String>, call: &EditCall) -> Option<String> {
    let input = call.input.as_ref()?;
    match call.tool_name.as_str() {
        "Write" => Some(input.get("content")?.as_str()?.to_owned()),
        "Edit" => apply_edit(text?, input),
        "MultiEdit" => input
            .get("edits")?
            .as_array()?
            .iter()
            .try_fold(text?, apply_edit),
        _ => None,
    }
}

pub fn explains_the_change(calls: &[&EditCall], before: Option<&str>, after: &str) -> bool {
    let mut ordered: Vec<&EditCall> = calls.to_vec();
    ordered.sort_by_key(|c| (c.at, c.tool_use_id));
    ordered
        .into_iter()
        .try_fold(before.map(str::to_owned), |text, call| {
            apply_call(text, call).map(Some)
        })
        .flatten()
        .is_some_and(|result| result == after)
}

fn finished_by(call: &EditCall, at: DateTime<Utc>) -> bool {
    call.failed != Some(true) && call.ran_to.is_some_and(|to| to <= at)
}

pub fn proven_late(
    calls: &[EditCall],
    at: DateTime<Utc>,
    before: Option<&str>,
    after: &str,
) -> Option<Authorship> {
    let mut finished: Vec<&EditCall> = calls
        .iter()
        .filter(|c| finished_by(c, at))
        .filter(|c| written_texts(c).is_some_and(|texts| texts.iter().all(|t| after.contains(t))))
        .collect();
    let sessions: BTreeSet<i64> = finished.iter().map(|c| c.session_id).collect();
    if sessions.len() != 1 {
        return None;
    }
    finished.sort_by_key(|c| (c.at, c.tool_use_id));
    (0..finished.len())
        .rev()
        .map(|start| &finished[start..])
        .find(|newest| explains_the_change(newest, before, after))
        .and_then(edit_authorship)
}

#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

pub struct FileChange<'a> {
    pub absolute_path: &'a str,
    pub relative_path: &'a str,
    pub before_sha: Option<&'a str>,
    pub after_sha: Option<&'a str>,
    pub edit: Window,
    pub shell: Window,
}

pub async fn of_file_change(
    persist: &PersistHandle,
    git: &Git,
    change: &FileChange<'_>,
) -> Result<Authorship, PersistError> {
    let late = Duration::milliseconds(LATE_ROW_MS);
    let calls = persist
        .tool_calls_naming_path(
            change.absolute_path,
            change.edit.from,
            change.edit.to + late,
        )
        .await?;
    let runs = persist
        .shell_commands_running(git.root(), change.shell.from, change.shell.to)
        .await?;
    if let Some(who) = proven_by_content(git, &calls, &runs, change).await {
        return Ok(who);
    }
    let paths = (git.root(), change.relative_path, change.absolute_path);
    if let Some(who) = super::author_named::named_shell(persist, paths, change.shell).await? {
        return Ok(who);
    }
    let naming: Vec<ToolCallNaming> = calls
        .iter()
        .filter(|c| c.at <= change.edit.to)
        .map(EditCall::naming)
        .collect();
    match decide(&naming, &runs) {
        Authorship::Unknown { candidates } => {
            let owner =
                super::author_named::owner_among(persist, &candidates, change.relative_path)
                    .await?;
            Ok(owner.map_or(Authorship::Unknown { candidates }, |session| {
                Authorship::Shell {
                    session,
                    named_by: None,
                }
            }))
        }
        who => Ok(who),
    }
}

async fn blob_text(git: &Git, sha: Option<&str>) -> Option<String> {
    let sha = sha?;
    match git.run(&["cat-file", "blob", sha]).await {
        Ok(raw) => Some(String::from_utf8_lossy(&raw).into_owned()),
        Err(err) => {
            tracing::warn!(root = %git.root(), error = %err, "change record: a changed file's content could not be read");
            None
        }
    }
}

async fn proven_by_content(
    git: &Git,
    calls: &[EditCall],
    runs: &[ShellRun],
    change: &FileChange<'_>,
) -> Option<Authorship> {
    let at = change.edit.to;
    if !calls
        .iter()
        .any(|c| ran_at(c, at) && written_texts(c).is_some())
    {
        return proven_late_by_content(git, calls, change).await;
    }
    let after = blob_text(git, change.after_sha).await?;
    let who = proven_edit(calls, at, &after)?;
    let proven = proven_calls(calls, at, &after);
    let session = who.session()?;
    let others: BTreeSet<i64> = runs
        .iter()
        .map(|r| r.session_id)
        .filter(|s| *s != session)
        .collect();
    if others.is_empty() {
        return Some(who);
    }
    let before = blob_text(git, change.before_sha).await;
    if explains_the_change(&proven, before.as_deref(), &after) {
        return Some(who);
    }
    Some(Authorship::Unknown {
        candidates: others
            .into_iter()
            .chain([session])
            .collect::<BTreeSet<i64>>()
            .into_iter()
            .collect(),
    })
}

async fn proven_late_by_content(
    git: &Git,
    calls: &[EditCall],
    change: &FileChange<'_>,
) -> Option<Authorship> {
    let at = change.edit.to;
    if !calls
        .iter()
        .any(|c| finished_by(c, at) && written_texts(c).is_some())
    {
        return None;
    }
    let after = blob_text(git, change.after_sha).await?;
    let before = blob_text(git, change.before_sha).await;
    proven_late(calls, at, before.as_deref(), &after)
}

pub async fn of_shell(
    persist: &PersistHandle,
    checkout: &str,
    shell: Window,
) -> Result<Authorship, PersistError> {
    let runs = persist
        .shell_commands_running(checkout, shell.from, shell.to)
        .await?;
    Ok(from_shell_runs(&runs))
}

pub async fn of_seat(
    persist: &PersistHandle,
    seat: Option<uuid::Uuid>,
) -> Result<Option<Authorship>, PersistError> {
    let Some(uuid) = seat else {
        return Ok(None);
    };
    Ok(persist
        .session_identity_by_uuid(uuid)
        .await?
        .map(|identity| Authorship::Seat {
            session: identity.id,
        }))
}
