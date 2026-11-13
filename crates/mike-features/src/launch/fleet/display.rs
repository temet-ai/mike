use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::collections::BTreeMap;
use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};

use super::definition::FleetDefinition;
use super::here;
use super::shutdown::{self, RUNNING, STOPPING};
use super::status::{FleetStatus, SeatStatus, assemble};
use crate::base_state::BaseState;
use crate::launch::{BackendId, Notice, TerminalBackend, backend_for};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

const INTERVAL: Duration = Duration::from_secs(20);
const REPLACE_AT: i64 = 600_000;

pub fn spawn_fleet_display(state: &BaseState) {
    super::retrospective_worker::spawn(state);
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let state = state.clone();
    let shutdown = state.shutdown();
    state.tracker.clone().spawn(async move {
        let mut ticker = interval(INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut seen = Seen::default();
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = ticker.tick() => tick(&state, &persist, &mut seen).await,
            }
        }
    });
}

#[derive(Default)]
struct Seen {
    verdicts: BTreeMap<String, usize>,
    blocked: BTreeMap<String, usize>,
    over: BTreeMap<String, Vec<String>>,
    quota: BTreeMap<String, Vec<String>>,
}

async fn tick(state: &BaseState, persist: &PersistHandle, seen: &mut Seen) {
    let Ok(fleets) = persist
        .fleet()
        .list_fleets_in_states(&[RUNNING, STOPPING])
        .await
    else {
        return;
    };
    let fleets = here::on_this_machine(fleets, here::checkout_present);
    let backend = state
        .config
        .launch
        .backend
        .parse::<BackendId>()
        .ok()
        .and_then(backend_for);
    drain_stopping(state, persist, backend.as_deref(), &fleets).await;
    let running = still_running(state, persist, backend.as_deref(), &fleets).await;
    watch_running(state, &running);
    for row in &running {
        super::gate_queue::queue(state, persist, row).await;
    }
    let Some(backend) = backend else {
        return;
    };
    for row in &running {
        let Ok(status) = assemble(state, row).await else {
            continue;
        };
        run_registered_steps(state, persist, row, &status).await;
        show(backend.as_ref(), persist, row, &status).await;
        announce(backend.as_ref(), row, &status, seen).await;
    }
}

async fn run_registered_steps(
    state: &BaseState,
    persist: &PersistHandle,
    row: &FleetRow,
    status: &FleetStatus,
) {
    let (Ok(def), Some(run_id)) = (FleetDefinition::from_value(&row.definition), &row.run_id)
    else {
        return;
    };
    let ctx = super::tick::TickCtx {
        state,
        persist,
        row,
        status,
        def: &def,
        run_id,
    };
    super::tick::run_steps(&ctx).await;
}

fn watch_running(state: &BaseState, running: &[FleetRow]) {
    let live: std::collections::BTreeSet<&str> =
        running.iter().filter_map(|r| r.run_id.as_deref()).collect();
    for run in super::watch::watched() {
        if !live.contains(run.as_str()) {
            super::watch::stop(&run);
        }
    }
    for row in running {
        if let (Some(run), Ok(def)) = (
            row.run_id.as_deref(),
            FleetDefinition::from_value(&row.definition),
        ) {
            super::watch::ensure(state, run, &def.repo);
        }
    }
}

async fn drain_stopping(
    state: &BaseState,
    persist: &PersistHandle,
    backend: Option<&dyn TerminalBackend>,
    fleets: &[FleetRow],
) {
    for row in fleets.iter().filter(|row| row.state == STOPPING) {
        match shutdown::stop(state, persist, row).await {
            Ok(report) if report.state == shutdown::DONE => announce_end(backend, row).await,
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, fleet = %row.slug, "fleet: stopping reconciliation remains blocked")
            }
        }
    }
}

async fn still_running(
    state: &BaseState,
    persist: &PersistHandle,
    backend: Option<&dyn TerminalBackend>,
    fleets: &[FleetRow],
) -> Vec<FleetRow> {
    let mut running = Vec::new();
    for row in fleets.iter().filter(|f| f.state == RUNNING) {
        match shutdown::reconcile(state, persist, row).await {
            Ok(true) => {
                announce_end(backend, row).await;
                continue;
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, fleet = %row.slug, "fleet: seat reconcile failed"),
        }
        if let Ok(Some(fresh)) = persist
            .fleet()
            .find_fleet(&row.fleet_uuid.to_string())
            .await
            && fresh.state == RUNNING
            && fresh.run_id == row.run_id
        {
            running.push(fresh);
        }
    }
    running
}

async fn announce_end(backend: Option<&dyn TerminalBackend>, row: &FleetRow) {
    if let Some(backend) = backend {
        backend
            .notify(&Notice {
                title: format!("{}: done", row.slug),
                body: "every seat has exited, so mike ended the fleet".to_string(),
                urgent: false,
            })
            .await;
    }
}

async fn show(
    backend: &dyn TerminalBackend,
    persist: &PersistHandle,
    row: &FleetRow,
    status: &FleetStatus,
) {
    let Some(run) = &row.run_id else {
        return;
    };
    let Ok(seats) = persist.fleet().fleet_seat_rows(run).await else {
        return;
    };
    for seat in seats.iter().filter(|s| s.closed_at.is_none()) {
        let (Some(name), Some(pane)) = (&seat.seat, &seat.surface_ref) else {
            continue;
        };
        let task = status
            .tasks
            .iter()
            .find(|t| t.seat.as_deref() == Some(name))
            .map(|t| t.id.clone())
            .unwrap_or_default();
        let values = vec![
            ("seat".to_string(), name.clone()),
            ("fleet".to_string(), seat_line(&task, seat.context_tokens)),
        ];
        backend.report_pane(pane, &values).await;
    }
    if let Ok(Some(run_row)) = persist.launch().find_run(run).await
        && let Some(workspace) = run_row.workspace_ref
    {
        let values = vec![("progress".to_string(), progress_line(status))];
        backend.report_workspace(&workspace, &values).await;
    }
}

pub fn seat_line(task: &str, tokens: Option<i64>) -> String {
    let size = match tokens {
        Some(t) if t >= REPLACE_AT => format!("{}k replace", t / 1000),
        Some(t) => format!("{}k", t / 1000),
        None => "no requests".to_string(),
    };
    if task.is_empty() {
        size
    } else {
        format!("{task} · {size}")
    }
}

pub fn progress_line(status: &FleetStatus) -> String {
    let passed = status
        .tasks
        .iter()
        .filter(|t| t.state == "passed" || t.state == "done")
        .count();
    let over = status
        .seats
        .iter()
        .filter(|s| !s.closed && s.context_tokens.is_some_and(|t| t >= REPLACE_AT))
        .count();
    let mut line = format!("{passed} of {} passed", status.tasks.len());
    if !status.needs_operator.is_empty() {
        line.push_str(&format!(" · {} need you", status.needs_operator.len()));
    }
    if let Some(seconds) = status.every_seat_idle_seconds {
        line.push_str(&format!(" · every seat idle {}m", seconds / 60));
    }
    if !status.blocked.is_empty() {
        line.push_str(&format!(" · {} blocked", status.blocked.len()));
    }
    if over > 0 {
        line.push_str(&format!(" · {over} over 600k"));
    }
    line
}

async fn announce(
    backend: &dyn TerminalBackend,
    row: &FleetRow,
    status: &FleetStatus,
    seen: &mut Seen,
) {
    let key = row.fleet_uuid.to_string();
    let fails: Vec<&_> = status
        .verdicts
        .iter()
        .filter(|v| v.verdict == "fail")
        .collect();
    let known = seen.verdicts.insert(key.clone(), status.verdicts.len());
    if let (Some(known), Some(newest)) = (known, status.verdicts.first())
        && status.verdicts.len() > known
        && newest.verdict == "fail"
        && fails.first().map(|f| f.at) == Some(newest.at)
    {
        backend
            .notify(&Notice {
                title: format!("{}: FAIL", row.slug),
                body: newest.title.clone(),
                urgent: true,
            })
            .await;
    }
    let blocked_known = seen.blocked.insert(key.clone(), status.blocked.len());
    if let (Some(known), Some(newest)) = (blocked_known, status.blocked.last())
        && status.blocked.len() > known
    {
        backend
            .notify(&Notice {
                title: format!("{}: {} is blocked", row.slug, newest.seat),
                body: newest.title.clone(),
                urgent: true,
            })
            .await;
    }
    let over: Vec<String> = status
        .seats
        .iter()
        .filter(|s| !s.closed && s.context_tokens.is_some_and(|t| t >= REPLACE_AT))
        .map(|s| s.seat.clone())
        .collect();
    notify_quota(backend, row, status, &mut seen.quota, &key).await;
    let before = seen.over.insert(key, over.clone()).unwrap_or_default();
    for seat in over.iter().filter(|s| !before.contains(s)) {
        let size = size_of(status, seat);
        backend
            .notify(&Notice {
                title: format!("{}: {seat} is over 600k", row.slug),
                body: format!("{size}k of 1000k; replace the seat"),
                urgent: false,
            })
            .await;
    }
}

async fn notify_quota(
    backend: &dyn crate::launch::TerminalBackend,
    row: &crate::store::persistence::fleet::FleetRow,
    status: &FleetStatus,
    seen: &mut BTreeMap<String, Vec<String>>,
    key: &str,
) {
    let stopped: Vec<String> = status
        .seats
        .iter()
        .filter(|s| s.state == "quota")
        .map(|s| s.seat.clone())
        .collect();
    let before = seen
        .insert(key.to_string(), stopped.clone())
        .unwrap_or_default();
    for seat in stopped.iter().filter(|s| !before.contains(s)) {
        backend
            .notify(&Notice {
                title: format!("{}: {seat} hit its usage limit", row.slug),
                body: "the seat stopped mid-task; its last request was refused with 429"
                    .to_string(),
                urgent: true,
            })
            .await;
    }
}

fn size_of(status: &FleetStatus, seat: &str) -> i64 {
    status
        .seats
        .iter()
        .find(|s| s.seat == seat)
        .and_then(|s: &SeatStatus| s.context_tokens)
        .unwrap_or(0)
        / 1000
}
