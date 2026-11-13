use crate::store::persistence::fleet_repo::FleetPersist as _;
use serde_json::json;

use super::brief_notes::fleet_tag;
use super::definition::FleetDefinition;
use super::events::EventKind;
use super::plan::ControlSeats;
use super::run_notebook::{self, RunNote};
use super::status_types::FleetStatus;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

pub const CLOSE_TAG: &str = "close";
const PASSED: &str = "passed";

async fn stop_requested_by(persist: &PersistHandle, run: &str) -> Option<String> {
    let events = persist.fleet().fleet_events_since(run, 0).await.ok()?;
    events
        .iter()
        .rev()
        .find(|event| event.kind == EventKind::StopRequested)
        .and_then(|event| event.payload.get("by")?.as_str().map(str::to_string))
}

struct Tasks {
    done: Vec<String>,
    open: Vec<String>,
}

fn tasks_of(status: &FleetStatus) -> Tasks {
    let (done, open): (Vec<_>, Vec<_>) = status.tasks.iter().partition(|t| t.state == PASSED);
    Tasks {
        done: done.iter().map(|t| t.id.clone()).collect(),
        open: open
            .iter()
            .map(|t| format!("{} ({})", t.id, t.state))
            .collect(),
    }
}

fn list(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}

fn when(row: &FleetRow) -> String {
    row.finished_at
        .map_or_else(|| "not recorded".to_string(), |at| at.to_rfc3339())
}

fn tasks_lines(status: Option<&FleetStatus>) -> (String, Option<Tasks>) {
    let Some(status) = status else {
        return (
            "Tasks: not read, the fleet's status could not be assembled at close.".to_string(),
            None,
        );
    };
    let tasks = tasks_of(status);
    let accepted = if status.accepted {
        "Acceptance: every task was accepted."
    } else {
        "Acceptance: not every task was accepted."
    };
    let lines = format!(
        "{accepted}\nTasks done ({}): {}\nTasks open ({}): {}",
        tasks.done.len(),
        list(&tasks.done),
        tasks.open.len(),
        list(&tasks.open)
    );
    (lines, Some(tasks))
}

fn body(row: &FleetRow, run: &str, stop: Option<&str>, tasks: &str) -> String {
    format!(
        "Fleet {slug} (fleet {fleet}), run {run}, is {state}.\nClosed at: {when}\nStop requested by: {stop}\n{tasks}",
        slug = row.slug,
        fleet = row.fleet_uuid,
        state = row.state,
        when = when(row),
        stop = stop.unwrap_or("not recorded"),
    )
}

pub async fn record(persist: &PersistHandle, row: &FleetRow, status: Option<&FleetStatus>) {
    let (Some(run), Ok(def)) = (
        row.run_id.as_deref(),
        FleetDefinition::from_value(&row.definition),
    ) else {
        return;
    };
    let coordinator = ControlSeats::recorded(&def).map(|seats| seats.coordinator);
    let Some(session) = run_notebook::session(persist, coordinator, &def.repo, run).await else {
        return;
    };
    let stop = stop_requested_by(persist, run).await;
    let (tasks_text, tasks) = tasks_lines(status);
    let note = RunNote {
        title: format!("Fleet {} closed: run {run}", row.slug),
        body: body(row, run, stop.as_deref(), &tasks_text),
        tags: vec![
            fleet_tag(&row.fleet_uuid.to_string()),
            run.to_string(),
            CLOSE_TAG.to_string(),
        ],
        data: json!({
            "run": run,
            "state": row.state,
            "closed_at": row.finished_at,
            "stop_requested_by": stop,
            "tasks_done": tasks.as_ref().map(|t| t.done.clone()),
            "tasks_open": tasks.as_ref().map(|t| t.open.clone()),
        }),
    };
    run_notebook::write_once(persist, session, note).await;
}
