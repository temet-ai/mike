use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::events::{EventPayload, NewFleetEvent, StopRequested};
use super::owned_execution;
use crate::launch::kill_for_server;
use crate::state::AppState;
use crate::store::persistence::fleet::{FleetRow, UnsettledFleetLaunch};
use crate::store::persistence::{PersistError, PersistHandle};
#[path = "seat_exit.rs"]
pub(super) mod seat_exit;

pub const DONE: &str = "done";
pub const RUNNING: &str = "running";
pub const STOPPING: &str = "stopping";
const EXITED: &str = "process exited";
const STOPPED: &str = "fleet stop";
const NEVER_STARTED: &str = "never started";
const START_GRACE_SECONDS: f64 = 300.0;
const DRAIN_WAIT: Duration = Duration::from_secs(2);
const STOP_ATTEMPTS: usize = 3;
const UNTRACKED: &str = "Only Mike-owned command groups and recorded seat root processes are checked; untracked descendants remain unknown.";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sweep {
    pub exited: Vec<i64>,
    pub live: usize,
}

pub fn sweep(seats: &[(i64, Option<i32>)], alive: impl Fn(i32) -> bool) -> Sweep {
    let mut found = Sweep::default();
    for (id, pid) in seats {
        match pid {
            Some(pid) if *pid > 0 && !alive(*pid) => found.exited.push(*id),
            _ => found.live += 1,
        }
    }
    found
}

pub async fn reconcile(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
) -> Result<bool, PersistError> {
    let fleet_db = persist.fleet();
    let fresh = current(persist, row).await?;
    if fresh.state != RUNNING {
        return Ok(fresh.state == DONE);
    }
    let Some(run) = &fresh.run_id else {
        return Ok(false);
    };
    let seats = fleet_db.unsettled_fleet_launches(run).await?;
    for seat in seats.iter().filter(|seat| seat.opened) {
        if matches!(seat_exit::observe(persist, run, seat).await, Ok(Some(true))) {
            fleet_db
                .record_fleet_launch_verified_exit(seat.id, run, EXITED)
                .await?;
            seat_exit::forget(run, seat);
        }
    }
    if !fleet_db.unsettled_fleet_launches(run).await?.is_empty()
        || left_unchecked(state, persist, &fresh).await
    {
        return Ok(false);
    }
    Ok(stop(state, persist, &fresh).await?.state == DONE)
}

async fn left_unchecked(state: &AppState, persist: &PersistHandle, row: &FleetRow) -> bool {
    let Some(commit) = super::gate::full::unchecked_at_close(&state.tracker, persist, row).await
    else {
        return false;
    };
    tracing::info!(fleet = %row.slug, %commit, "fleet: every seat has exited, but the branch's newest commit has no passing gate yet, so the fleet stays open");
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopReport {
    pub fleet: Uuid,
    pub slug: String,
    pub state: String,
    pub seats: Vec<SeatStop>,
    #[serde(default)]
    pub managed: ManagedStop,
    #[serde(default)]
    pub untracked_descendants: String,
    #[serde(default)]
    pub root_origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedStop {
    #[serde(default = "containment_capability")]
    pub containment: String,
    pub known_in_this_process: bool,
    pub stop_requested: bool,
    pub pending_owners: Vec<String>,
    pub pending_receipts: Vec<i64>,
}

fn containment_capability() -> String {
    super::managed_containment_capability().to_string()
}

impl Default for ManagedStop {
    fn default() -> Self {
        Self {
            containment: containment_capability(),
            known_in_this_process: false,
            stop_requested: false,
            pending_owners: Vec::new(),
            pending_receipts: Vec::new(),
        }
    }
}

impl ManagedStop {
    fn settled(&self) -> bool {
        self.pending_owners.is_empty() && self.pending_receipts.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatStop {
    pub seat: Option<String>,
    pub session: Option<Uuid>,
    pub closed: bool,
    pub note: Option<String>,
    #[serde(default)]
    pub launch_id: i64,
    #[serde(default)]
    pub process_exited: Option<bool>,
}

pub async fn stop(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
) -> Result<StopReport, PersistError> {
    let stopped = request_stop(persist, row).await?;
    if stopped.state == DONE {
        super::status_pane::close(state, &stopped).await;
        return recorded_done_report(persist, &stopped).await;
    }
    let seats = match &stopped.run_id {
        Some(run) => stop_run(state, persist, run).await?,
        None => Vec::new(),
    };
    let mut report = settled_report(persist, &stopped, seats).await?;
    if report.managed.settled() {
        let remaining = match stopped.run_id.as_deref() {
            Some(run) => !persist
                .fleet()
                .unsettled_fleet_launches(run)
                .await?
                .is_empty(),
            None => false,
        };
        if !remaining {
            let fresh = finish_stop(persist, &stopped).await?;
            if fresh.state == DONE {
                super::status_pane::close(state, &fresh).await;
                record_close(state, persist, &fresh).await;
            }
            report.state = fresh.state;
        }
    }
    Ok(report)
}

async fn record_close(state: &AppState, persist: &PersistHandle, row: &FleetRow) {
    let status = super::status::assemble(state, row).await.ok();
    super::close_note::record(persist, row, status.as_ref()).await;
}

pub const ABANDON_AFTER_SECONDS: f64 = 7200.0 + 600.0;

async fn stop_run(
    state: &AppState,
    persist: &PersistHandle,
    run: &str,
) -> Result<Vec<SeatStop>, PersistError> {
    owned_execution::registry().drain(run, DRAIN_WAIT).await;
    settle_unrun(persist, run).await?;
    abandon_expired(persist, run).await?;
    let mut seats = Vec::new();
    for seat in persist.fleet().unsettled_fleet_launches(run).await? {
        seats.push(stop_seat(state, persist, run, &seat).await);
    }
    Ok(seats)
}

async fn settle_unrun(persist: &PersistHandle, run: &str) -> Result<(), PersistError> {
    let registry = owned_execution::registry();
    for owner in registry.unrun_receipts(run) {
        if let Some(receipt) = owner.receipt {
            let note = format!(
                "\nAbandoned at fleet stop: mike could not record this command's receipt ({}) and no process it started is still running. Its result is unknown.\n",
                owner.detail.as_deref().unwrap_or("receipt blocked")
            );
            let abandoned = persist
                .fleet()
                .abandon_unrun_fleet_check(receipt, run, &note)
                .await?;
            tracing::warn!(%run, receipt, abandoned, "fleet stop: settled a command receipt that no process is running");
        }
        registry.forget_unrun(run, owner.id);
    }
    Ok(())
}

async fn abandon_expired(persist: &PersistHandle, run: &str) -> Result<(), PersistError> {
    let note = "\nAbandoned at fleet stop: no finish was recorded within the 7200 s limit every managed command runs under, so no process on any machine can still be running it (the mike that started it most likely restarted). Its result is unknown.\n";
    let abandoned = persist
        .fleet()
        .abandon_expired_fleet_checks(run, ABANDON_AFTER_SECONDS, note)
        .await?;
    if !abandoned.is_empty() {
        tracing::warn!(%run, ?abandoned, "fleet stop: recorded checks past the command limit as abandoned");
    }
    Ok(())
}

async fn request_stop(persist: &PersistHandle, row: &FleetRow) -> Result<FleetRow, PersistError> {
    for _ in 0..STOP_ATTEMPTS {
        let before = current(persist, row).await?;
        let key = before
            .run_id
            .clone()
            .unwrap_or_else(|| before.fleet_uuid.to_string());
        let fence = owned_execution::registry().fence(&key).await;
        let fresh = current(persist, &before).await?;
        if fresh.run_id != before.run_id {
            continue;
        }
        if fresh.state == DONE {
            return Ok(fresh);
        }
        if fresh.state == STOPPING {
            fence.stopping_persisted();
            return Ok(fresh);
        }
        if !(matches!(fresh.state.as_str(), "starting" | RUNNING)
            || fresh.state == "failed" && fresh.run_id.is_some())
        {
            return Err(PersistError::DataInvariant(format!(
                "fleet {} in state {} cannot be stopped",
                fresh.slug, fresh.state
            )));
        }
        if let Some(stopped) = persist
            .fleet()
            .transition_fleet_state(&fresh, STOPPING, None, None)
            .await?
        {
            fence.stopping_persisted();
            record_stop_requested(persist, &stopped).await;
            return Ok(stopped);
        }
    }
    Err(PersistError::DataInvariant(format!(
        "fleet {} changed while stopping; retry",
        row.slug
    )))
}

pub(super) fn stop_requested_event(run: &str) -> NewFleetEvent {
    NewFleetEvent {
        run_id: run.to_owned(),
        task: None,
        session_uuid: None,
        payload: EventPayload::StopRequested(StopRequested {
            by: "mike fleet stop".to_owned(),
        }),
    }
}

async fn record_stop_requested(persist: &PersistHandle, row: &FleetRow) {
    let Some(run) = row.run_id.as_deref() else {
        return;
    };
    if let Err(err) = persist
        .fleet()
        .append_fleet_event(&stop_requested_event(run))
        .await
    {
        tracing::warn!(error = %err, %run, "fleet: could not record the stop request");
    }
}

async fn current(persist: &PersistHandle, row: &FleetRow) -> Result<FleetRow, PersistError> {
    let fresh = persist
        .fleet()
        .find_fleet(&row.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| {
            PersistError::DataInvariant(format!("fleet {} disappeared", row.fleet_uuid))
        })?;
    if row.run_id.is_some() && fresh.run_id != row.run_id {
        return Err(PersistError::DataInvariant(format!(
            "fleet {} changed runs; retry with its current state",
            row.slug
        )));
    }
    Ok(fresh)
}

async fn stop_seat(
    state: &AppState,
    persist: &PersistHandle,
    run: &str,
    seat: &UnsettledFleetLaunch,
) -> SeatStop {
    let mut report = SeatStop {
        seat: seat.seat.clone(),
        session: seat.session_uuid,
        closed: false,
        note: None,
        launch_id: seat.id,
        process_exited: seat_exit::observe(persist, run, seat).await.unwrap_or(None),
    };
    if report.process_exited != Some(true) {
        match seat.session_uuid {
            Some(session) => match kill_for_server(state, session).await {
                Ok(killed) => {
                    report.closed = killed.closed;
                    report.process_exited =
                        seat_exit::observe(persist, run, seat).await.unwrap_or(None);
                    report.note = killed.note;
                }
                Err(error) => report.note = Some(error.to_string()),
            },
            None => report.note = Some("the launch recorded no session id".into()),
        }
    }
    if report.process_exited == Some(true) {
        if let Err(error) = persist
            .fleet()
            .record_fleet_launch_verified_exit(seat.id, run, STOPPED)
            .await
        {
            report.note = Some(format!(
                "root exit observed, but closure could not be recorded: {error}"
            ));
        } else {
            seat_exit::forget(run, seat);
        }
    } else if !close_if_never_started(persist, run, seat, &mut report).await {
        add_origin_diagnostic(&mut report);
    }
    report
}

async fn close_if_never_started(
    persist: &PersistHandle,
    run: &str,
    seat: &UnsettledFleetLaunch,
    report: &mut SeatStop,
) -> bool {
    if seat.opened_at.is_some() || seat.surface_ref.is_some() {
        return false;
    }
    match persist
        .fleet()
        .close_never_started_fleet_launch(seat.id, run, NEVER_STARTED, START_GRACE_SECONDS)
        .await
    {
        Ok(true) => {
            report.closed = true;
            report.process_exited = Some(true);
            report.note = Some(format!(
                "{NEVER_STARTED}: the launch never recorded a pane or an open time, and its session has no request or has ended, so there is no process to wait for"
            ));
            true
        }
        Ok(false) => false,
        Err(error) => {
            report.note = Some(format!(
                "the launch looks never started, but closing it failed: {error}"
            ));
            false
        }
    }
}

fn add_origin_diagnostic(report: &mut SeatStop) {
    let detail = format!(
        "seat root exit is unverified ({}); observe the matching live session on its originating host; after restart, an already-ended root needs additional durable origin/exit evidence",
        crate::launch::local_root::capability()
    );
    report.note = Some(match report.note.take() {
        Some(note) => format!("{note}; {detail}"),
        None => detail,
    });
}

async fn recorded_done_report(
    persist: &PersistHandle,
    row: &FleetRow,
) -> Result<StopReport, PersistError> {
    let unresolved = match row.run_id.as_deref() {
        Some(run) => persist.fleet().unsettled_fleet_launches(run).await?,
        None => Vec::new(),
    };
    let seats = unresolved.into_iter().map(|seat| SeatStop {
        launch_id: seat.id,
        seat: seat.seat,
        session: seat.session_uuid,
        closed: false,
        process_exited: seat.close_process_exited,
        note: Some("historical done fleet has an unresolved launch receipt; no new shutdown action was taken".into()),
    }).collect();
    settled_report(persist, row, seats).await
}

async fn settled_report(
    persist: &PersistHandle,
    row: &FleetRow,
    seats: Vec<SeatStop>,
) -> Result<StopReport, PersistError> {
    let mut managed = ManagedStop::default();
    if let Some(run) = &row.run_id {
        let live = owned_execution::registry().snapshot(run);
        managed.known_in_this_process = live.known_in_this_process;
        managed.stop_requested = live.stop_requested;
        managed.pending_owners = live
            .pending
            .iter()
            .map(|owner| {
                format!(
                    "{} {} {:?}: {}; containment={}; admission_root={:?}; owned_processes={}",
                    owner.id,
                    owner.ledger,
                    owner.phase,
                    owner.detail.as_deref().unwrap_or("settling"),
                    owner.containment,
                    owner.admission_root,
                    owner.owned_processes
                )
            })
            .collect();
        managed.pending_receipts = persist
            .fleet()
            .pending_fleet_commands(run)
            .await?
            .into_iter()
            .map(|row| row.id)
            .collect();
    }
    Ok(StopReport {
        fleet: row.fleet_uuid,
        slug: row.slug.clone(),
        state: row.state.clone(),
        seats,
        managed,
        untracked_descendants: UNTRACKED.into(),
        root_origin: crate::launch::local_root::capability().into(),
    })
}

async fn finish_stop(persist: &PersistHandle, row: &FleetRow) -> Result<FleetRow, PersistError> {
    let fresh = current(persist, row).await?;
    if fresh.state != STOPPING {
        return Ok(fresh);
    }
    let retrospective = match fresh.run_id.as_deref() {
        Some(run) => super::retrospective::stage(persist, &fresh, run).await?,
        None => false,
    };
    let staged = current(persist, &fresh).await?;
    let Some(done) = persist.fleet().complete_fleet_run(&staged, None).await? else {
        return current(persist, &staged).await;
    };
    if let Some(run) = done.run_id.as_deref() {
        super::run_record::close_record(persist, &done, run).await;
        retire_checkout(persist, &done, run, retrospective).await;
    }
    Ok(done)
}

async fn retire_checkout(persist: &PersistHandle, row: &FleetRow, run: &str, retrospective: bool) {
    if let Ok(def) = super::definition::FleetDefinition::from_value(&row.definition) {
        super::warm::retire(persist, &def, run).await;
        if !retrospective {
            super::cleanup::after_close(persist, row, &def, run).await;
        }
    }
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;
