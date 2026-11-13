use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::Value;
use uuid::Uuid;

use super::report_judge::{ReportToJudge, judge_report, tell_seat};
use super::{LeafError, LeafModel};
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::events::{EventKind, FleetEvent};
use crate::launch::fleet::floor::Outcome;
use crate::launch::fleet::report::{Exercised, REPORT_KIND, REPORT_TAG};
use crate::launch::fleet::seated::Seated;
use crate::launch::fleet::told;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{NoteOrder, SessionNoteFilter};

pub(crate) const JUDGES_PER_TICK: usize = 2;
const GIVE_UP_AFTER: u32 = 5;
const REPORTS_READ: i64 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pending {
    pub(crate) task: String,
    pub(crate) note: Uuid,
    pub(crate) seat: Uuid,
    pub(crate) reported_at: DateTime<Utc>,
}

fn note_uuid(event: &FleetEvent) -> Option<Uuid> {
    event.payload.get("note_uuid")?.as_str()?.parse().ok()
}

fn newest_report<'a>(events: &'a [FleetEvent], task: &str) -> Option<&'a FleetEvent> {
    events
        .iter()
        .filter(|e| e.kind == EventKind::ReportFiled && e.task.as_deref() == Some(task))
        .max_by_key(|e| (e.occurred_at, e.id))
}

fn judged(events: &[FleetEvent], note: Uuid) -> bool {
    events
        .iter()
        .any(|e| e.kind == EventKind::VerdictRecorded && note_uuid(e) == Some(note))
}

fn floor_since(events: &[FleetEvent], task: &str, since: DateTime<Utc>) -> Option<bool> {
    events
        .iter()
        .filter(|e| e.task.as_deref() == Some(task) && e.occurred_at >= since)
        .filter_map(|e| match e.kind {
            EventKind::FloorPassed => Some((e, true)),
            EventKind::FloorFailed => Some((e, false)),
            _ => None,
        })
        .max_by_key(|(e, _)| (e.occurred_at, e.id))
        .map(|(_, passed)| passed)
}

pub(crate) fn pending(def: &FleetDefinition, events: &[FleetEvent]) -> Vec<Pending> {
    let mut out = Vec::new();
    for (id, task) in &def.tasks {
        let Some(report) = newest_report(events, id) else {
            continue;
        };
        let (Some(note), Some(seat)) = (note_uuid(report), report.session_uuid) else {
            continue;
        };
        if judged(events, note) {
            continue;
        }
        if task.done_when.is_some() && floor_since(events, id, report.occurred_at) != Some(true) {
            continue;
        }
        out.push(Pending {
            task: id.clone(),
            note,
            seat,
            reported_at: report.occurred_at,
        });
    }
    out.sort_by_key(|p| p.reported_at);
    out
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

pub(crate) fn report_from(
    data: &Value,
    pending: &Pending,
    (run, fleet): (&str, &str),
) -> Option<ReportToJudge> {
    let sha = data.get("commit")?.as_str()?.trim().to_string();
    if sha.is_empty() {
        return None;
    }
    Some(ReportToJudge {
        run: run.to_string(),
        fleet: fleet.to_string(),
        task: pending.task.clone(),
        seat: pending.seat,
        note: pending.note,
        sha,
        criteria: strings(data.get("criteria")),
        exercised: data
            .get("exercised")
            .filter(|v| !v.is_null())
            .cloned()
            .and_then(|v| serde_json::from_value::<Exercised>(v).ok()),
        tests_changed: strings(data.get("tests_changed")),
        unestablished: strings(data.get("unestablished")),
    })
}

pub(crate) async fn report_data(persist: &PersistHandle, p: &Pending) -> Option<Value> {
    let filter = SessionNoteFilter {
        kind: Some(REPORT_KIND),
        status: None,
        tag: Some(REPORT_TAG),
        q: None,
        order: NoteOrder::NewestFirst,
        limit: REPORTS_READ,
        offset: 0,
    };
    persist
        .launch()
        .fetch_session_notes(p.seat, filter)
        .await
        .ok()?
        .into_iter()
        .find(|row| row.note_uuid == p.note)
        .map(|row| row.data)
}

fn gave_up_key(note: Uuid) -> String {
    format!("judge-gave-up:{note}")
}

fn attempts() -> &'static DashMap<Uuid, u32> {
    static ATTEMPTS: OnceLock<DashMap<Uuid, u32>> = OnceLock::new();
    ATTEMPTS.get_or_init(DashMap::new)
}

pub(crate) async fn pass(
    state: &AppState,
    persist: &PersistHandle,
    fleet: &str,
    def: &FleetDefinition,
    run: &str,
) {
    let Some(pending) = awaiting(persist, def, run).await else {
        return;
    };
    match super::model(state) {
        Some(model) => judge_pending(&model, persist, (def, run, fleet), &pending).await,
        None => no_model(persist, run, &pending).await,
    }
}

async fn awaiting(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
) -> Option<Vec<Pending>> {
    let events = persist.fleet().fleet_events_since(run, 0).await.ok()?;
    let launches = persist
        .fleet()
        .unsettled_fleet_launches(run)
        .await
        .unwrap_or_default();
    if Seated::from_record(def, &events, &launches).acceptance_seated(def) {
        return None;
    }
    let pending = pending(def, &events);
    (!pending.is_empty()).then_some(pending)
}

pub(crate) async fn judge_pending(
    model: &dyn LeafModel,
    persist: &PersistHandle,
    (def, run, fleet): (&FleetDefinition, &str, &str),
    pending: &[Pending],
) {
    let told_keys = persist.fleet().told_keys(run).await.unwrap_or_default();
    let open = pending
        .iter()
        .filter(|p| !told_keys.contains(&gave_up_key(p.note)))
        .take(JUDGES_PER_TICK);
    for p in open {
        let Some(data) = report_data(persist, p).await else {
            continue;
        };
        let Some(report) = report_from(&data, p, (run, fleet)) else {
            continue;
        };
        let floor = Outcome::Passed {
            sha: report.sha.clone(),
        };
        match judge_report(model, persist, def, &report, floor).await {
            Ok(()) => {
                attempts().remove(&p.note);
            }
            Err(LeafError::Refused(why)) => {
                tracing::debug!(run, task = %p.task, why, "fleet judge: the call was refused; next tick tries again");
                return;
            }
            Err(err) => failed(persist, run, p, &err).await,
        }
    }
}

async fn failed(persist: &PersistHandle, run: &str, p: &Pending, err: &LeafError) {
    let n: u32 = *attempts()
        .entry(p.note)
        .and_modify(|n| *n += 1)
        .or_insert(1);
    tracing::warn!(run, task = %p.task, attempt = n, error = %err, "fleet judge: the report was not judged");
    if n < GIVE_UP_AFTER {
        return;
    }
    attempts().remove(&p.note);
    told::mark(persist, run, &[gave_up_key(p.note)]).await;
    let body = format!(
        "[mike judge] Your report on task {} (note {}) could not be judged after {n} attempts; the last answer was: {err}. No verdict is recorded; report again to be judged afresh.",
        p.task, p.note
    );
    tell_seat(persist, p.seat, &body).await;
}

async fn no_model(persist: &PersistHandle, run: &str, pending: &[Pending]) {
    tracing::warn!(
        run,
        waiting = pending.len(),
        "fleet judge: no model is configured (the steerer model and its key on /settings); reports wait"
    );
    let told_keys = persist.fleet().told_keys(run).await.unwrap_or_default();
    for p in pending {
        let key = format!("judge-no-model:{}", p.note);
        if told_keys.contains(&key) {
            continue;
        }
        let body = format!(
            "[mike judge] Your report on task {} is not judged: mike has no judge model (the steerer model and its key on /settings). The report stands; it is judged once that is set.",
            p.task
        );
        tell_seat(persist, p.seat, &body).await;
        told::mark(persist, run, &[key]).await;
    }
}
