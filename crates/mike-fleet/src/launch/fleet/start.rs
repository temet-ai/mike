use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::path::PathBuf;

use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::plan::{ControlSeats, FleetFacts};
use super::status::assemble;
use super::status_pane::SEAT as STATUS_SEAT;
use super::{seat, staff, warm};
use crate::base_state::BaseState;
use crate::launch::{LaunchError, PaneRequest, RunSeat, Surface, backend_for};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::settings::RepoLocation;

const STATUS_REFRESH_SECONDS: &str = "5";
const DELETED_SUFFIX: &str = " (deleted)";

pub fn run_id_for(row: &FleetRow) -> String {
    let short: String = row
        .fleet_uuid
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect();
    format!("fleet-{}-{short}", row.slug)
}

pub fn begin(state: BaseState, fleet: Uuid) {
    let tracker = state.tracker.clone();
    tracker.spawn(async move {
        if let Err(problem) = start(&state, fleet).await {
            tracing::error!(%fleet, %problem, "mike could not start the fleet");
            if let Some(persist) = state.persist() {
                fail(persist, fleet, &problem).await;
            }
        }
    });
}

async fn fail(persist: &PersistHandle, fleet: Uuid, problem: &str) {
    let Ok(Some(row)) = persist.fleet().find_fleet(&fleet.to_string()).await else {
        return;
    };
    if row.state != "starting" {
        return;
    }
    let report = json!({"problem": problem});
    if let Err(err) = persist
        .fleet()
        .transition_fleet_state(&row, "failed", None, Some(&report))
        .await
    {
        tracing::error!(%fleet, error = %err, "mike could not record that the fleet failed to start");
    }
    super::start_failure::tell(persist, &row, problem).await;
}

async fn start(state: &BaseState, fleet: Uuid) -> Result<(), String> {
    let persist = state
        .persist()
        .ok_or_else(|| "the database is not reachable".to_string())?;
    let (mut def, facts) = prepare(state, persist, fleet).await?;
    super::start_baseline::establish(persist, &mut def, &facts, &state.tracker).await?;
    ensure_starting(persist, fleet, &facts.run_id).await?;
    open_comms_thread(persist, fleet).await;
    let (status, _) = status_pane(state, persist, &def, &facts).await;
    let mut report = vec![status];
    report.extend(open_rest(state, persist, (&def, &facts)).await);
    record_control_and_brief_notes(persist, fleet, &def, &facts).await?;
    let running = record(persist, fleet, &facts.run_id, "running", report).await?;
    super::gate_queue::queue(state, persist, &running).await;
    staff_now(state, persist, &running).await;
    Ok(())
}

async fn record_control_and_brief_notes(
    persist: &PersistHandle,
    fleet: Uuid,
    def: &FleetDefinition,
    facts: &FleetFacts,
) -> Result<(), String> {
    ensure_starting(persist, fleet, &facts.run_id).await?;
    if let Some(problem) = notebook_problem(def, &facts.seats) {
        return Err(problem.to_string());
    }
    if !seat::patch_definition(persist, fleet, &control_patch(&facts.seats)).await {
        return Err("fleet control-seat assignment changed during startup".into());
    }
    ensure_starting(persist, fleet, &facts.run_id).await?;
    if super::brief_notes::as_notes(def) {
        super::brief_notes_sync::ensure_run_part(
            persist,
            def,
            facts,
            std::time::Duration::from_secs(30),
        )
        .await;
    }
    Ok(())
}

async fn ensure_starting(persist: &PersistHandle, fleet: Uuid, run: &str) -> Result<(), String> {
    if matches!(persist.fleet().find_fleet(&fleet.to_string()).await, Ok(Some(row)) if row.state == "starting" && row.run_id.as_deref() == Some(run))
    {
        Ok(())
    } else {
        Err("fleet startup was cancelled".to_string())
    }
}

async fn open_comms_thread(persist: &PersistHandle, fleet: Uuid) {
    if let Err(err) = persist.fleet().ensure_fleet_comms_thread(fleet).await {
        tracing::warn!(%fleet, error = %err, "mike could not open the fleet's comms thread; seat mail stays unthreaded");
    }
}

async fn prepare(
    state: &BaseState,
    persist: &PersistHandle,
    fleet: Uuid,
) -> Result<(FleetDefinition, FleetFacts), String> {
    let row = persist
        .fleet()
        .find_fleet(&fleet.to_string())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("fleet {fleet} does not exist"))?;
    if row.state != "starting" {
        return Err("fleet startup was cancelled before preparation".into());
    }
    let mut def = FleetDefinition::from_value(&row.definition)?;
    resolve_repo_on_this_machine(state, persist, fleet, &mut def).await?;
    let run_id = run_id_for(&row);
    let fence = super::owned_execution::registry().fence(&run_id).await;
    if fence.is_stopping() {
        return Err("this fleet run has already been stopped".into());
    }
    persist
        .fleet()
        .initialize_fleet_run(&row, &run_record(state, &def, &run_id, None))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("fleet startup changed before its run was initialized")?;
    fence.establish_known_empty()?;
    drop(fence);
    super::cleanup::mark_run_clone(&def, std::path::Path::new(&row.project_path), &row.slug);
    let facts = FleetFacts {
        fleet: fleet.to_string(),
        slug: row.slug.clone(),
        run_id,
        seats: control_seats(persist, &def, row.created_by).await,
        paths: super::bundle::paths_for(&def.repo),
    };
    Ok((def, facts))
}

async fn control_seats(
    persist: &PersistHandle,
    def: &FleetDefinition,
    author: Option<Uuid>,
) -> ControlSeats {
    let recorded = ControlSeats::recorded(def);
    let coordinator = existing(persist, recorded.coordinator).await.or(author);
    let acceptance = existing(persist, recorded.acceptance).await;
    ControlSeats::answering(coordinator, acceptance)
}

async fn existing(persist: &PersistHandle, session: Option<Uuid>) -> Option<Uuid> {
    let session = session?;
    match persist.session_row_id(session).await {
        Ok(Some(_)) => Some(session),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(%session, error = %err, "mike could not check that a control seat's session exists; keeping it");
            Some(session)
        }
    }
}

async fn resolve_repo_on_this_machine(
    state: &BaseState,
    persist: &PersistHandle,
    fleet: Uuid,
    def: &mut FleetDefinition,
) -> Result<(), String> {
    let Some(machine) = state.resolve_host_machine_uuid().await else {
        return Ok(());
    };
    let location = persist
        .repo_location_on_machine(&def.repo, machine)
        .await
        .map_err(|error| error.to_string())?;
    let Some(repo) = repo_for_machine(&def.repo, machine, &location)? else {
        return Ok(());
    };
    if !seat::patch_definition(persist, fleet, &json!({"repo": repo})).await {
        return Err(format!(
            "the fleet's repo {} is {repo} on this machine, but the definition could not be updated",
            def.repo
        ));
    }
    tracing::info!(%fleet, from = %def.repo, to = %repo, "fleet repo resolved through this machine's project location");
    def.repo = repo;
    Ok(())
}

pub fn repo_for_machine(
    repo: &str,
    machine: Uuid,
    location: &RepoLocation,
) -> Result<Option<String>, String> {
    match (&location.own_root, location.known) {
        (Some(own), _) if own == repo => Ok(None),
        (Some(own), _) => Ok(Some(own.clone())),
        (None, true) => Err(format!(
            "this machine ({machine}) has no location for the project the fleet's repo {repo} \
             belongs to; register this machine's checkout with register_project_location, or \
             start the fleet on a machine that has one"
        )),
        (None, false) => Ok(None),
    }
}

async fn open_rest(
    state: &BaseState,
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
) -> Vec<Value> {
    if super::start_baseline::blocked(def) {
        return Vec::new();
    }
    let warming = warm::begin_all(state, persist, def, facts).await;
    let warmed = warm::finish_all(state, persist, def, facts, warming).await;
    let mut report: Vec<Value> = warm::start_reference(state, persist, def, facts, &warmed)
        .await
        .into_iter()
        .collect();
    report.extend(warmed);
    report
}

async fn record(
    persist: &PersistHandle,
    fleet: Uuid,
    run: &str,
    state: &str,
    report: Vec<Value>,
) -> Result<FleetRow, String> {
    let fresh = persist
        .fleet()
        .find_fleet(&fleet.to_string())
        .await
        .map_err(|error| error.to_string())?
        .ok_or("fleet disappeared while its launch was recorded")?;
    if fresh.state != "starting" || fresh.run_id.as_deref() != Some(run) {
        return Err("fleet startup was cancelled before its launch was recorded".into());
    }
    persist
        .fleet()
        .transition_fleet_state(&fresh, state, None, Some(&Value::Array(report)))
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "fleet changed while recording startup; its newer state was kept".into())
}

async fn staff_now(state: &BaseState, persist: &PersistHandle, running: &FleetRow) {
    match assemble(state, running).await {
        Ok(status) => staff::staff(state, persist, running, &status).await,
        Err(problem) => {
            tracing::warn!(fleet = %running.slug, %problem, "mike could not read the fleet it just started; workers start on the next pass");
        }
    }
}

pub fn control_patch(seats: &ControlSeats) -> Value {
    json!({
        "coordinator": {"session": seats.coordinator},
        "acceptance": {"session": seats.acceptance},
    })
}

pub fn notebook_problem(def: &FleetDefinition, seats: &ControlSeats) -> Option<&'static str> {
    (super::brief_notes::as_notes(def) && seats.coordinator.is_none()).then_some(
        "briefs.as_notes keeps the run's brief in a session's notebook, and this fleet has no session to hold it: create the fleet from a session (define_fleet records it) or set coordinator.session, then send ready again",
    )
}

fn run_record(
    state: &BaseState,
    def: &FleetDefinition,
    run_id: &str,
    workspace: Option<String>,
) -> NewRun {
    NewRun {
        run_id: run_id.to_string(),
        project_path: Some(def.repo.clone()),
        backend: state.config.launch.backend.clone(),
        workspace_ref: workspace,
        tab_ref: None,
    }
}

async fn status_pane(
    state: &BaseState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
) -> (Value, Option<String>) {
    match admitted_status_pane(state, persist, def, facts).await {
        Ok((surface, problem)) => {
            let created = (!surface.reused)
                .then_some(surface.workspace_ref.clone())
                .flatten();
            (super::status_pane::entry(&surface, problem), created)
        }
        Err(problem) => (
            json!({"seat": STATUS_SEAT, "opened": false, "problem": problem}),
            None,
        ),
    }
}

async fn admitted_status_pane(
    state: &BaseState,
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
) -> Result<(Surface, Option<String>), String> {
    let fence = super::owned_execution::registry()
        .fence(&facts.run_id)
        .await;
    if fence.is_stopping() {
        return Err("status pane refused: fleet stop was requested".into());
    }
    let held = persist
        .fleet()
        .begin_fleet_launch(&facts.run_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or("status pane has no exact fleet run")?;
    let surface = open_status_pane(state, def, facts)
        .await
        .map_err(|error| error.to_string())?;
    let problem = held
        .record_run(&run_record(
            state,
            def,
            &facts.run_id,
            surface.workspace_ref.clone(),
        ))
        .await
        .err()
        .map(|error| {
            format!("status pane opened, but its workspace could not be recorded: {error}")
        });
    Ok((surface, problem))
}

async fn open_status_pane(
    state: &BaseState,
    def: &FleetDefinition,
    facts: &FleetFacts,
) -> Result<Surface, LaunchError> {
    let id = state.config.launch.backend.parse()?;
    let backend = backend_for(id).ok_or(LaunchError::BackendNone)?;
    let mike = running_binary()?;
    let command = format!(
        "MIKE_INSPECT_ADDR='{}' '{}' fleet status {} --watch {STATUS_REFRESH_SECONDS}",
        state.config.inspect_listen_addr.replace('\'', "'\\''"),
        mike.to_string_lossy().replace('\'', "'\\''"),
        facts.fleet
    );
    let surface = backend
        .open_pane(&PaneRequest {
            folder: PathBuf::from(&def.repo),
            run: RunSeat {
                run_id: facts.run_id.clone(),
                workspace_ref: None,
                seat: Some(STATUS_SEAT.to_string()),
                role: Some(STATUS_SEAT.to_string()),
            },
            command,
        })
        .await?;
    Ok(surface)
}

pub fn running_binary() -> Result<PathBuf, LaunchError> {
    let exe = std::env::current_exe()
        .map_err(|e| LaunchError::io("finding the running mike binary", e))?;
    let text = exe.to_string_lossy();
    let path = PathBuf::from(text.trim_end_matches(DELETED_SUFFIX));
    if path.is_file() {
        return Ok(path);
    }
    Ok(PathBuf::from("mike"))
}

#[cfg(test)]
pub mod repo_for_machine_tests {
    use super::{RepoLocation, repo_for_machine};

    const MACHINE: uuid::Uuid = uuid::Uuid::from_u128(0x7d0a_6f8e_0000_4000_8000_0000_0000_0001);

    fn location(known: bool, own_root: Option<&str>) -> RepoLocation {
        RepoLocation {
            known,
            own_root: own_root.map(str::to_owned),
        }
    }

    #[test]
    fn repo_for_machine_rewrites_another_machines_path_to_this_machines_checkout() {
        let resolved = repo_for_machine(
            "/mnt/data/projects/temet/temet-proxy",
            MACHINE,
            &location(true, Some("/Users/dawid/projects/temet/temet-proxy")),
        );
        assert_eq!(
            resolved,
            Ok(Some("/Users/dawid/projects/temet/temet-proxy".to_owned()))
        );
    }

    #[test]
    fn repo_for_machine_keeps_this_machines_own_path_and_unknown_paths() {
        assert_eq!(
            repo_for_machine("/repo", MACHINE, &location(true, Some("/repo"))),
            Ok(None)
        );
        assert_eq!(
            repo_for_machine("/repo", MACHINE, &location(false, None)),
            Ok(None)
        );
    }

    #[test]
    fn repo_for_machine_refuses_a_known_project_this_machine_has_no_location_for() {
        let refused = repo_for_machine("/repo", MACHINE, &location(true, None));
        let message = refused.err().unwrap_or_default();
        assert!(message.contains("no location"), "{message}");
        assert!(message.contains(&MACHINE.to_string()), "{message}");
    }
}

#[cfg(test)]
#[path = "start_control_tests.rs"]
mod control_tests;
