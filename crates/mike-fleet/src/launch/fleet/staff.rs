use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::{
    ControlSeats, FleetFacts, SeatPlan, ready_tasks, worker, worker_seat_name, worker_with,
};
use super::status::FleetStatus;
use super::{seat, staff_claims};
use crate::base_state::BaseState;
use crate::launch::{BackendId, backend_for};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetRow, FleetSeatRow};
use crate::store::persistence::launch_repo::LaunchPersist as _;

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
    staff_claims::settle(run_id, &unstarted, std::time::Instant::now());
    let adopted = adopt_live_launches(persist, run_id).await;
    let held = staff_claims::held_by(run_id);
    let ready: Vec<String> = ready_tasks(&def, &released)
        .into_iter()
        .filter(|id| unstarted.contains(id) && !held.contains(id))
        .filter(|id| !is_adopted(&def, id, &adopted))
        .collect();
    let ready = super::start_baseline::staffable(persist, row, &def, ready).await;
    if ready.is_empty() || memory_holds(row, &def, ready.len()).await {
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
    let live = super::staff_limits::live_worker_seats(status);
    let room = room.min(super::staff_limits::seat_room(&def, live, held.len()));
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

pub(super) fn claim_all(
    (def, facts): (&FleetDefinition, &FleetFacts),
    tasks: impl Iterator<Item = String>,
) -> Vec<(String, Uuid)> {
    let mut claimed = Vec::new();
    for task in tasks {
        let Some(plan) = worker(def, facts, &task, Uuid::new_v4()) else {
            continue;
        };
        if staff_claims::begin(&facts.run_id, &task, plan.session) {
            claimed.push((task, plan.session));
        }
    }
    claimed
}

async fn memory_holds(row: &FleetRow, def: &FleetDefinition, tasks: usize) -> bool {
    let Some(hold) = super::staff_limits::memory_hold(def).await else {
        return false;
    };
    tracing::warn!(fleet = %row.fleet_uuid, tasks, %hold, "fleet: ready tasks wait for host memory");
    true
}

async fn adopt_live_launches(persist: &PersistHandle, run_id: &str) -> BTreeSet<String> {
    match persist.fleet().adopt_live_fleet_launches(run_id).await {
        Ok(adopted) => adopted
            .into_iter()
            .inspect(|launch| {
                tracing::warn!(launch = launch.id, seat = ?launch.seat, "fleet: a seat is sending requests but its launch was never marked opened; mike marked it opened");
            })
            .filter_map(|launch| launch.seat)
            .collect(),
        Err(error) => {
            tracing::warn!(%error, run = run_id, "fleet: live launches could not be checked");
            BTreeSet::new()
        }
    }
}

fn is_adopted(def: &FleetDefinition, task: &str, adopted: &BTreeSet<String>) -> bool {
    def.tasks
        .get(task)
        .is_some_and(|t| adopted.contains(&worker_seat_name(&t.discipline, task)))
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

async fn seat_may_be_live(persist: &PersistHandle, session: Uuid) -> bool {
    persist
        .launch()
        .find_launch_by_session(session)
        .await
        .ok()
        .flatten()
        .is_some_and(|row| {
            row.result != "failed" || row.reason.as_deref() == Some(seat::OPEN_UNCONFIRMED)
        })
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
            if let Some(task) = plan.task.as_deref()
                && !seat_may_be_live(&persist, plan.session).await
            {
                staff_claims::end_session(&facts.run_id, task, plan.session);
            }
        }
        if let (true, Some(task)) = (super::brief_notes::as_notes(&def), plan.task.as_deref()) {
            let seat = (&def, &facts);
            super::brief_notes_sync::record_task_part(&persist, seat, (task, plan.session)).await;
        }
    });
}
