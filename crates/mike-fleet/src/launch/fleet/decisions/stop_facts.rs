use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::check::{asker, is_running};
use super::super::commit::covers;
use super::super::definition::FleetDefinition;
use super::super::seated::Seated;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

const CHANGES_READ: i64 = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastCheck {
    pub passed: bool,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StopFacts {
    pub seat: String,
    pub task: String,
    pub uncommitted: Vec<String>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub check_passed_after_change: bool,
    pub last_check: Option<LastCheck>,
    pub open_asks: Vec<String>,
    pub check_running: bool,
    pub done_when: Option<Value>,
}

impl StopFacts {
    pub fn unchecked_changes(&self) -> bool {
        !self.uncommitted.is_empty() && !self.check_passed_after_change
    }

    pub fn state(&self, final_message: &str) -> Value {
        json!({
            "task": self.task,
            "done_when": self.done_when,
            "uncommitted_files": self.uncommitted.len(),
            "check_passed_after_last_change": self.check_passed_after_change,
            "a_check_is_running": self.check_running,
            "open_questions_the_seat_asked": self.open_asks,
            "final_message": final_message,
        })
    }
}

fn last_check_of(row: Option<crate::store::persistence::fleet::TreeCheckRow>) -> Option<LastCheck> {
    row.filter(|r| r.finished_at.is_some()).map(|r| LastCheck {
        passed: r.passed(),
        finished_at: r.finished_at,
    })
}

pub fn passed_after(check: Option<&LastCheck>, change: Option<DateTime<Utc>>) -> bool {
    match (check, change) {
        (Some(check), Some(change)) => {
            check.passed && check.finished_at.is_some_and(|done| done >= change)
        }
        (Some(check), None) => check.passed,
        (None, _) => false,
    }
}

async fn open_asks(persist: &PersistHandle, run: &str, session: Uuid) -> Vec<String> {
    match persist.launch().open_questions_in_run(run).await {
        Ok(rows) => rows
            .into_iter()
            .filter(|q| q.asker == session)
            .map(|q| q.question)
            .collect(),
        Err(_) => Vec::new(),
    }
}

async fn last_change(
    persist: &PersistHandle,
    run: &str,
    dirty: &[String],
) -> Option<DateTime<Utc>> {
    let changes = persist
        .fleet()
        .tree_changes_since(run, DateTime::<Utc>::MIN_UTC, None, CHANGES_READ)
        .await
        .ok()?;
    changes
        .iter()
        .filter(|c| dirty.contains(&c.path))
        .map(|c| c.seen_at)
        .max()
}

pub async fn gather(
    persist: &PersistHandle,
    (def, seated): (&FleetDefinition, &Seated),
    run: &str,
    session: Uuid,
) -> Option<StopFacts> {
    let task = seated.task_of(session)?.to_owned();
    let who = asker(def, seated, session)?;
    let dirty: Vec<String> = super::super::commit::changed_paths(&def.repo)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|path| who.everything || covers(&who.owned, path))
        .collect();
    let last_change_at = last_change(persist, run, &dirty).await;
    let last_check = last_check_of(persist.fleet().latest_tree_check(run).await.ok().flatten());
    let done_when = def
        .tasks
        .get(&task)
        .and_then(|t| t.done_when.as_ref())
        .and_then(|d| serde_json::to_value(d).ok());
    Some(StopFacts {
        seat: who.seat,
        check_passed_after_change: passed_after(last_check.as_ref(), last_change_at),
        task,
        uncommitted: dirty,
        last_change_at,
        last_check,
        open_asks: open_asks(persist, run, session).await,
        check_running: is_running(run),
        done_when,
    })
}

#[cfg(test)]
#[path = "stop_facts_tests.rs"]
mod tests;
