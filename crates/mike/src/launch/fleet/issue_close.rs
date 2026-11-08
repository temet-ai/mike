use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::status::{FleetStatus, TaskStatus};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{
    NewSessionNote, NoteOrder, SessionNoteFilter, SessionNoteUpdate,
};

pub const KIND: &str = "issues";
const CLOSED: &str = "closed";
const NOTES_READ: i64 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub note: Uuid,
    pub task: String,
    pub sha: String,
    pub issues: Vec<u32>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Close {
    pub note: Uuid,
    pub issue: u32,
    pub comment: String,
}

#[async_trait]
pub trait Tracker: Send + Sync {
    async fn is_open(&self, issue: u32) -> Result<bool, String>;
    async fn close(&self, issue: u32, comment: &str) -> Result<(), String>;
}

pub struct Gh<'a> {
    pub repo: &'a str,
}

impl Gh<'_> {
    async fn run(&self, args: &[&str]) -> Result<String, String> {
        let output = tokio::process::Command::new("gh")
            .args(args)
            .current_dir(self.repo)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|err| format!("could not run gh: {err}"))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[async_trait]
impl Tracker for Gh<'_> {
    async fn is_open(&self, issue: u32) -> Result<bool, String> {
        let number = issue.to_string();
        let state = self
            .run(&["issue", "view", &number, "--json", "state", "-q", ".state"])
            .await?;
        Ok(state.eq_ignore_ascii_case("open"))
    }

    async fn close(&self, issue: u32, comment: &str) -> Result<(), String> {
        let number = issue.to_string();
        self.run(&["issue", "close", &number, "--comment", comment])
            .await
            .map(|_| ())
    }
}

pub fn passed(task: &TaskStatus) -> bool {
    !task.criteria.is_empty()
        && task.passed_criteria.len() == task.criteria.len()
        && task
            .last
            .as_ref()
            .is_some_and(|last| last.verdict == "pass")
}

pub fn due(tasks: &[TaskStatus], recorded: &[Recorded]) -> Vec<Close> {
    recorded
        .iter()
        .filter(|record| !record.closed)
        .filter_map(|record| {
            let task = tasks.iter().find(|task| task.id == record.task)?;
            passed(task).then_some((record, task))
        })
        .flat_map(|(record, task)| {
            let comment = format!(
                "Closed by mike: acceptance passed task {} ({}); the commit was {}.",
                task.id,
                task.name,
                record.sha.get(..10).unwrap_or(&record.sha)
            );
            record.issues.iter().map(move |issue| Close {
                note: record.note,
                issue: *issue,
                comment: comment.clone(),
            })
        })
        .collect()
}

pub async fn settle(tracker: &dyn Tracker, closes: &[Close]) -> Vec<(Close, Result<bool, String>)> {
    let mut done = Vec::new();
    for close in closes {
        let result = match tracker.is_open(close.issue).await {
            Ok(false) => Ok(false),
            Ok(true) => tracker
                .close(close.issue, &close.comment)
                .await
                .map(|()| true),
            Err(why) => Err(why),
        };
        done.push((close.clone(), result));
    }
    done
}

pub async fn record(persist: &PersistHandle, session: Uuid, task: &str, sha: &str, issues: &[u32]) {
    if issues.is_empty() {
        return;
    }
    let listed: Vec<String> = issues.iter().map(|issue| format!("#{issue}")).collect();
    let title = format!("{task} refs {} for acceptance to close", listed.join(" "));
    let tags = vec![KIND.to_string(), task.to_string()];
    let data = json!({"task": task, "sha": sha, "issues": issues});
    let note = NewSessionNote {
        kind: Some(KIND),
        title: &title,
        body: None,
        status: Some("open"),
        tags: Some(&tags),
        data: Some(&data),
    };
    if let Err(err) = persist.create_session_note(session, note).await {
        tracing::warn!(session_id = %session, error = %err, "fleet: could not record the issues a commit refers to");
    }
}

fn recorded_of(row: &crate::store::persistence::session_notes::SessionNoteRow) -> Option<Recorded> {
    let issues: Vec<u32> = row
        .data
        .get("issues")?
        .as_array()?
        .iter()
        .filter_map(|value| value.as_u64().and_then(|n| u32::try_from(n).ok()))
        .collect();
    Some(Recorded {
        note: row.note_uuid,
        task: row.data.get("task")?.as_str()?.to_string(),
        sha: row.data.get("sha").and_then(Value::as_str)?.to_string(),
        issues,
        closed: row.status.as_deref() == Some(CLOSED),
    })
}

async fn recorded_for(persist: &PersistHandle, session: Uuid) -> Vec<Recorded> {
    let filter = SessionNoteFilter {
        kind: Some(KIND),
        status: Some("open"),
        tag: None,
        q: None,
        order: NoteOrder::OldestFirst,
        limit: NOTES_READ,
        offset: 0,
    };
    match persist.fetch_session_notes(session, filter).await {
        Ok(rows) => rows.iter().filter_map(recorded_of).collect(),
        Err(err) => {
            tracing::warn!(session_id = %session, error = %err, "fleet: could not read the issues a seat's commits refer to");
            Vec::new()
        }
    }
}

pub async fn tick(persist: &PersistHandle, def: &FleetDefinition, status: &FleetStatus) {
    let gh = Gh { repo: &def.repo };
    for task in status.tasks.iter().filter(|task| passed(task)) {
        let Some(session) = def
            .tasks
            .get(&task.id)
            .and_then(|task| task.session.as_deref())
            .and_then(|id| id.parse::<Uuid>().ok())
        else {
            continue;
        };
        let recorded = recorded_for(persist, session).await;
        let closes = due(&status.tasks, &recorded);
        if closes.is_empty() {
            continue;
        }
        let done = settle(&gh, &closes).await;
        finish(persist, &recorded, &done).await;
    }
}

async fn finish(
    persist: &PersistHandle,
    recorded: &[Recorded],
    done: &[(Close, Result<bool, String>)],
) {
    for record in recorded {
        let mine: Vec<_> = done.iter().filter(|(c, _)| c.note == record.note).collect();
        if mine.is_empty() {
            continue;
        }
        report_failures(&mine);
        if mine.iter().all(|(_, result)| result.is_ok()) {
            mark_closed(persist, record.note).await;
        }
    }
}

fn report_failures(mine: &[&(Close, Result<bool, String>)]) {
    for (close, result) in mine {
        if let Err(why) = result {
            tracing::warn!(issue = close.issue, %why, "fleet: could not close an issue after acceptance passed its task");
        }
    }
}

async fn mark_closed(persist: &PersistHandle, note: Uuid) {
    let update = SessionNoteUpdate {
        status: Some(CLOSED),
        ..SessionNoteUpdate::default()
    };
    if let Err(err) = persist.update_session_note(note, update).await {
        tracing::warn!(error = %err, "fleet: could not mark a note's issues closed");
    }
}
