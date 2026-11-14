use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{LazyLock, Mutex, PoisonError};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::{
    ControlSeats, FleetFacts, SeatPlan, ready_tasks, worker, worker_seat_name, worker_with,
};
use super::seat;
use super::status::FleetStatus;
use crate::base_state::BaseState;
use crate::launch::{BackendId, backend_for};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetRow, FleetSeatRow};

const CACHE_MINUTES: i64 = 55;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub task: String,
    pub waiting_for: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Siblings {
    Warm,
    Warming(String),
    NoneStarted,
}

fn answered(row: &FleetSeatRow, now: DateTime<Utc>) -> bool {
    row.last_status.is_some()
        && row
            .last_request_at
            .is_some_and(|at| now - at < Duration::minutes(CACHE_MINUTES))
}

fn siblings(
    def: &FleetDefinition,
    discipline: &str,
    seats: &[FleetSeatRow],
    now: DateTime<Utc>,
) -> Siblings {
    let mut warming = None;
    for id in def
        .tasks
        .iter()
        .filter(|(_, t)| t.discipline == discipline)
        .map(|(id, _)| id)
    {
        let name = worker_seat_name(discipline, id);
        let Some(row) = seats
            .iter()
            .rev()
            .find(|r| r.seat.as_deref() == Some(name.as_str()))
        else {
            continue;
        };
        match row {
            row if answered(row, now) => return Siblings::Warm,
            row if row.closed_at.is_some() || row.last_status.is_some() => {}
            _ => {
                warming.get_or_insert(name);
            }
        }
    }
    warming.map_or(Siblings::NoneStarted, Siblings::Warming)
}

pub fn warm_order(
    def: &FleetDefinition,
    ready: &[String],
    seats: &[FleetSeatRow],
    now: DateTime<Utc>,
) -> (Vec<String>, Vec<Hold>) {
    let mut due: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
    for id in ready {
        if let Some(task) = def.tasks.get(id) {
            due.entry(task.discipline.as_str()).or_default().push(id);
        }
    }
    let mut go = BTreeSet::new();
    let mut held = Vec::new();
    for (discipline, tasks) in due {
        let (first, rest) = match siblings(def, discipline, seats, now) {
            Siblings::Warm => (None, tasks.as_slice()),
            Siblings::Warming(seat) => {
                held.extend(tasks.iter().map(|t| hold(t, &seat)));
                continue;
            }
            Siblings::NoneStarted => match tasks.split_first() {
                Some((first, rest)) => (Some(*first), rest),
                None => continue,
            },
        };
        match first {
            Some(first) => {
                go.insert(first.clone());
                let seat = worker_seat_name(discipline, first);
                held.extend(rest.iter().map(|t| hold(t, &seat)));
            }
            None => go.extend(rest.iter().map(|t| (*t).clone())),
        }
    }
    let start = ready
        .iter()
        .filter(|id| go.contains(*id))
        .cloned()
        .collect();
    (start, held)
}

fn hold(task: &str, seat: &str) -> Hold {
    Hold {
        task: task.to_string(),
        waiting_for: seat.to_string(),
    }
}

async fn order_siblings(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run_id: &str,
    ready: Vec<String>,
) -> Vec<String> {
    if !def.staffing.warm_siblings {
        return ready;
    }
    let seats = persist
        .fleet()
        .fleet_seat_rows(run_id)
        .await
        .unwrap_or_default();
    let (start, held) = warm_order(def, &ready, &seats, Utc::now());
    for hold in &held {
        tracing::debug!(task = %hold.task, waiting_for = %hold.waiting_for, "fleet: a fork waits for its warm sibling");
    }
    start
}

pub fn passed_tasks(status: &FleetStatus) -> BTreeSet<String> {
    status
        .tasks
        .iter()
        .filter(|task| task.ready)
        .map(|task| task.id.clone())
        .collect()
}

static STARTING_NOW: LazyLock<Mutex<BTreeSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(BTreeSet::new()));

fn begin_start(run: &str, task: &str) -> bool {
    STARTING_NOW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert((run.to_string(), task.to_string()))
}

fn end_start(run: &str, task: &str) {
    STARTING_NOW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&(run.to_string(), task.to_string()));
}

pub fn unstarted_tasks(status: &FleetStatus) -> BTreeSet<String> {
    status
        .tasks
        .iter()
        .filter(|task| task.state == "queued")
        .map(|task| task.id.clone())
        .collect()
}

pub async fn staff(
    state: &BaseState,
    persist: &PersistHandle,
    row: &FleetRow,
    status: &FleetStatus,
) {
    let (Ok(def), Some(run_id)) = (FleetDefinition::from_value(&row.definition), &row.run_id)
    else {
        return;
    };
    super::propose_tick::tick(persist, row, &def).await;
    super::issue_close::tick(persist, &def, status).await;
    let seats = ControlSeats::recorded(&def);
    let released = super::release::released(persist, &def, status).await;
    let unstarted = unstarted_tasks(status);
    let ready: Vec<String> = ready_tasks(&def, &released)
        .into_iter()
        .filter(|id| unstarted.contains(id))
        .collect();
    let ready = super::start_baseline::staffable(persist, row, &def, ready).await;
    if ready.is_empty() {
        return;
    }
    let ready = order_siblings(persist, &def, run_id, ready).await;
    let facts = FleetFacts {
        fleet: row.fleet_uuid.to_string(),
        slug: row.slug.clone(),
        run_id: run_id.clone(),
        seats,
        paths: super::bundle::paths_for(&def.repo),
    };
    super::run_text_notes::record(persist, (&def, &facts)).await;
    let (room, surfaces) = room(state).await;
    if ready.len() > room {
        tell_no_room(row.fleet_uuid, &ready[room..], &surfaces);
    }
    let claimed = claim_all((&def, &facts), ready.into_iter().take(room));
    let run_note = if super::brief_notes::as_notes(&def) {
        super::brief_notes_sync::run_note_id(persist, &facts).await
    } else {
        None
    };
    for (task, session) in claimed {
        if let Some(plan) = worker_with(&def, &facts, &task, session, run_note) {
            start(state, persist, &def, &facts, plan);
        }
    }
}

fn claim_all(
    (def, facts): (&FleetDefinition, &FleetFacts),
    tasks: impl Iterator<Item = String>,
) -> Vec<(String, Uuid)> {
    let mut claimed = Vec::new();
    for task in tasks {
        let Some(plan) = worker(def, facts, &task, Uuid::new_v4()) else {
            continue;
        };
        if begin_start(&facts.run_id, &task) {
            claimed.push((task, plan.session));
        }
    }
    claimed
}

async fn room(state: &BaseState) -> (usize, String) {
    let limit = usize::try_from(state.config.launch.max_surfaces).unwrap_or(usize::MAX);
    if limit == 0 {
        return (usize::MAX, String::new());
    }
    let backend = state
        .config
        .launch
        .backend
        .parse::<BackendId>()
        .ok()
        .and_then(backend_for);
    let Some(backend) = backend else {
        return (0, "no terminal backend is configured".into());
    };
    match backend.list().await {
        Ok(open) => (
            limit.saturating_sub(open.len()),
            no_room_state(open.len(), limit),
        ),
        Err(_) => (usize::MAX, String::new()),
    }
}

pub fn no_room_state(open: usize, limit: usize) -> String {
    format!(
        "waiting: no free terminal surface ({open} of {limit} open); close a finished pane or raise launch_max_surfaces"
    )
}

fn tell_no_room(fleet: Uuid, tasks: &[String], surfaces: &str) {
    tracing::warn!(%fleet, tasks = tasks.len(), %surfaces, "fleet: ready tasks get no seat");
}

fn start(
    state: &BaseState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    plan: SeatPlan,
) {
    let (state, persist, def, facts) = (state.clone(), persist.clone(), def.clone(), facts.clone());
    state.tracker.clone().spawn(async move {
        let report = seat::launch(&state, &persist, &def, &facts.run_id, &plan).await;
        if !seat::opened(&report) {
            tracing::warn!(seat = %plan.seat, problem = %seat::problem(&report), "mike could not start a worker for a ready task");
        }
        if let Some(task) = plan.task.as_deref() {
            end_start(&facts.run_id, task);
        }
        if let (true, Some(task)) = (super::brief_notes::as_notes(&def), plan.task.as_deref()) {
            let seat = (&def, &facts);
            super::brief_notes_sync::record_task_part(&persist, seat, (task, plan.session)).await;
        }
    });
}
