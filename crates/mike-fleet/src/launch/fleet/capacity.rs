use std::time::Duration;

use serde::Serialize;

use super::build_waits::BuildWaits;
use super::definition::FleetDefinition;
use super::managed_queue::{self, Depth, Lane, Waiting};
use super::{host_memory, staff_claims};
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueuedCheck {
    pub receipt: i64,
    pub seat: String,
    pub lane: &'static str,
    pub place: usize,
    pub expected_wait_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaneLoad {
    pub lane: &'static str,
    pub command: String,
    pub slots: usize,
    pub in_flight: usize,
    pub queued: usize,
    pub measured_run_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeasuredBuilds {
    pub commands: usize,
    pub lock_wait_minutes: f64,
    pub unit_wait_minutes: f64,
    pub compile_minutes: f64,
    pub wait_minutes_per_compile_minute: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Capacity {
    pub seats_live: usize,
    pub seats_starting: usize,
    pub seat_limit: Option<usize>,
    pub builds_in_flight: usize,
    pub builds_queued: usize,
    pub free_memory_gb: Option<f64>,
    pub min_free_memory_gb: Option<u32>,
    pub lanes: Vec<LaneLoad>,
    pub queue: Vec<QueuedCheck>,
    pub measured_builds: Option<MeasuredBuilds>,
}

pub struct LaneInput {
    pub lane: Lane,
    pub command: String,
    pub executing: usize,
    pub waiting: Vec<Waiting>,
    pub measured: Option<Duration>,
}

pub struct Inputs<'a> {
    pub def: &'a FleetDefinition,
    pub seats_live: usize,
    pub seats_starting: usize,
    pub free_memory_gb: Option<f64>,
    pub lanes: Vec<LaneInput>,
    pub waits: BuildWaits,
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn queued_checks(input: &LaneInput) -> Vec<QueuedCheck> {
    let slots = input.lane.slots();
    input
        .waiting
        .iter()
        .enumerate()
        .map(|(ahead, queued)| QueuedCheck {
            receipt: queued.receipt,
            seat: queued.seat.clone(),
            lane: input.lane.name(),
            place: ahead + 1,
            expected_wait_seconds: input.measured.map(|measured| {
                managed_queue::expected_wait(
                    Depth {
                        waiting: ahead,
                        executing: input.executing,
                        slots,
                    },
                    measured,
                )
                .as_secs()
            }),
        })
        .collect()
}

fn lane_load(input: &LaneInput) -> LaneLoad {
    LaneLoad {
        lane: input.lane.name(),
        command: input.command.clone(),
        slots: input.lane.slots(),
        in_flight: input.executing,
        queued: input.waiting.len(),
        measured_run_seconds: input.measured.map(|measured| measured.as_secs()),
    }
}

fn measured_builds(waits: BuildWaits) -> Option<MeasuredBuilds> {
    (waits.commands > 0).then(|| MeasuredBuilds {
        commands: waits.commands,
        lock_wait_minutes: round1(waits.lock_wait_minutes),
        unit_wait_minutes: round1(waits.unit_wait_minutes),
        compile_minutes: round1(waits.compile_minutes),
        wait_minutes_per_compile_minute: (waits.compile_minutes > 0.0).then(|| {
            round1((waits.lock_wait_minutes + waits.unit_wait_minutes) / waits.compile_minutes)
        }),
    })
}

pub fn report(inputs: Inputs<'_>) -> Capacity {
    let queue = inputs.lanes.iter().flat_map(queued_checks).collect();
    Capacity {
        seats_live: inputs.seats_live,
        seats_starting: inputs.seats_starting,
        seat_limit: inputs.def.staffing.max_seats,
        builds_in_flight: inputs.lanes.iter().map(|lane| lane.executing).sum(),
        builds_queued: inputs.lanes.iter().map(|lane| lane.waiting.len()).sum(),
        free_memory_gb: inputs.free_memory_gb.map(round1),
        min_free_memory_gb: inputs.def.staffing.min_free_memory_gb,
        lanes: inputs.lanes.iter().map(lane_load).collect(),
        queue,
        measured_builds: measured_builds(inputs.waits),
    }
}

async fn lane_input(
    persist: &PersistHandle,
    def: &FleetDefinition,
    (lane, command): (Lane, &str),
) -> Option<LaneInput> {
    if command.trim().is_empty() {
        return None;
    }
    let (executing, waiting) = managed_queue::waiting_in(&def.repo, lane);
    Some(LaneInput {
        lane,
        command: command.trim().to_string(),
        executing,
        waiting,
        measured: managed_queue::measure(persist, command).await,
    })
}

pub async fn gather(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    seats_live: usize,
) -> Capacity {
    let mut lanes = Vec::new();
    for lane in [
        (Lane::Check, &def.check.command),
        (Lane::Gate, &def.gate.command),
    ] {
        lanes.extend(lane_input(persist, def, (lane.0, lane.1)).await);
    }
    let timings = super::build_timing::read_run(&super::seat_settings::timings_dir(def), run);
    report(Inputs {
        def,
        seats_live,
        seats_starting: staff_claims::held_by(run).len(),
        free_memory_gb: host_memory::free_gb().await,
        lanes,
        waits: super::build_waits::build_waits(&timings),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedOperation {
    pub command: String,
    pub measured_run_seconds: Option<u64>,
    pub capacity: Option<u32>,
    pub queue_minutes_if_every_seat_runs_it_once: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Planned {
    pub seats_at_once: usize,
    pub seat_limit: Option<usize>,
    pub free_memory_gb: Option<f64>,
    pub min_free_memory_gb: Option<u32>,
    pub operations: Vec<PlannedOperation>,
}

pub fn planned_operation(
    def: &FleetDefinition,
    command: &str,
    measured: Option<Duration>,
    seats: usize,
) -> PlannedOperation {
    let capacity = def
        .execution
        .operation(command)
        .and_then(|op| op.resources.iter().map(|claim| claim.capacity()).min());
    let queue = match (measured, capacity) {
        (Some(measured), Some(capacity)) if capacity > 0 => Some(round1(
            measured.as_secs_f64() * seats as f64 / f64::from(capacity) / 60.0,
        )),
        _ => None,
    };
    PlannedOperation {
        command: command.trim().to_string(),
        measured_run_seconds: measured.map(|measured| measured.as_secs()),
        capacity,
        queue_minutes_if_every_seat_runs_it_once: queue,
    }
}

pub async fn plan(persist: Option<&PersistHandle>, def: &FleetDefinition, seats: usize) -> Planned {
    let mut operations = Vec::new();
    for command in [&def.check.command, &def.gate.command] {
        if command.trim().is_empty() {
            continue;
        }
        let measured = match persist {
            Some(persist) => managed_queue::measure(persist, command).await,
            None => None,
        };
        operations.push(planned_operation(def, command, measured, seats));
    }
    Planned {
        seats_at_once: seats,
        seat_limit: def.staffing.max_seats,
        free_memory_gb: host_memory::free_gb().await.map(round1),
        min_free_memory_gb: def.staffing.min_free_memory_gb,
        operations,
    }
}

#[cfg(test)]
#[path = "capacity_tests.rs"]
mod tests;
