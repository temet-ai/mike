use crate::store::persistence::fleet_repo::FleetPersist as _;
use uuid::Uuid;

use super::brief_notes::{as_notes, run_tags, task_part, task_tags};
use super::brief_notes_sync::{StoredPart, latest_part};
use super::brief_version::{GivenPart, facts_for, parts_line, task_of};
use super::definition::FleetDefinition;
use super::plan_briefs::WORKER_PURPOSE;
use crate::store::persistence::PersistHandle;

pub const LEAD: &str =
    "Your brief follows, copied from mike session notes. No brief file was written for it.";

pub fn is_delivered(prompt: &str) -> bool {
    prompt.starts_with(LEAD)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPart {
    pub note: Option<Uuid>,
    pub version: i64,
    pub body: String,
}

fn run_head(run: &StoredPart) -> String {
    format!("mike session note {}, version {}", run.note, run.version)
}

fn task_head(task_id: &str, task: &TaskPart) -> String {
    match task.note {
        Some(note) => format!("mike session note {note}, version {}", task.version),
        None => format!(
            "mike session note tagged task:{task_id} and part:task in your own notebook, version {}",
            task.version
        ),
    }
}

pub fn compose(
    (seat, slug, task_id): (&str, &str, &str),
    run: &StoredPart,
    task: &TaskPart,
) -> String {
    let given = [
        GivenPart {
            part: "run".to_string(),
            note: Some(run.note),
            version: run.version,
        },
        GivenPart {
            part: "task".to_string(),
            note: task.note,
            version: task.version,
        },
    ];
    format!(
        "{LEAD}\n\n{WORKER_PURPOSE}\n\nYou are seat {seat} of fleet {slug}, a worker.\n\n== Run part: {run_head} ==\n{run_body}\n\n== Task part: {task_head} ==\n{task_body}\n\n{parts}",
        run_head = run_head(run),
        run_body = run.body.trim(),
        task_head = task_head(task_id, task),
        task_body = task.body.trim(),
        parts = parts_line(&given),
    )
}

async fn task_part_for(
    persist: &PersistHandle,
    def: &FleetDefinition,
    (fleet, task_id): (&str, &str),
) -> Option<TaskPart> {
    let session = def
        .tasks
        .get(task_id)?
        .session
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    if let Some(session) = session
        && let Some(stored) = latest_part(persist, session, &task_tags(fleet, task_id)).await
    {
        return Some(TaskPart {
            note: Some(stored.note),
            version: stored.version,
            body: stored.body,
        });
    }
    Some(TaskPart {
        note: None,
        version: 1,
        body: task_part(def, task_id)?,
    })
}

pub async fn first_message(persist: &PersistHandle, run: &str, seat: &str) -> Option<String> {
    let row = persist.fleet().find_fleet_by_run(run).await.ok()??;
    let def = FleetDefinition::from_value(&row.definition).ok()?;
    if !as_notes(&def) {
        return None;
    }
    let facts = facts_for(&row, &def)?;
    let task_id = task_of(&def, seat)?;
    let run_part = latest_part(persist, facts.seats.coordinator, &run_tags(&facts.fleet)).await?;
    let task = task_part_for(persist, &def, (&facts.fleet, task_id)).await?;
    Some(compose((seat, &facts.slug, task_id), &run_part, &task))
}
