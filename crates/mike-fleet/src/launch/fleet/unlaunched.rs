//! Whether a running fleet still has ready work that no seat has been started
//! for. Reconcile ends a fleet once every seat has exited; between closing the
//! templates and launching the first workers a fleet has no open seat at all,
//! and it must not be ended in that gap.

use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::BTreeSet;

use super::definition::FleetDefinition;
use super::plan::worker_seat_name;
use super::staff::{Waiting, startable, unstarted_tasks};
use crate::base_state::BaseState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

/// The queued tasks whose `after` tasks are released and that have never had a
/// worker seat in this run. A task whose seat was launched and has exited is
/// left out, so a fleet whose seats were all closed still ends.
pub fn never_started(
    def: &FleetDefinition,
    released: &BTreeSet<String>,
    unstarted: &BTreeSet<String>,
    launched_seats: &BTreeSet<String>,
) -> Vec<String> {
    let none_held = BTreeSet::new();
    startable(
        def,
        released,
        (unstarted, &none_held),
        &mut Waiting::default(),
    )
    .into_iter()
    .filter(|id| {
        def.tasks
            .get(id)
            .is_some_and(|task| !launched_seats.contains(&worker_seat_name(&task.discipline, id)))
    })
    .collect()
}

/// True while a seat is being started for the run, or while a ready task has
/// never had a seat. A failure to read the fleet counts as nothing waiting, so
/// a fleet mike cannot read still ends as it did before.
pub async fn waiting_to_start(state: &BaseState, persist: &PersistHandle, row: &FleetRow) -> bool {
    let Some(run) = row.run_id.as_deref() else {
        return false;
    };
    if !super::staff_claims::held_by(run).is_empty() {
        return true;
    }
    let waiting = unstaffed(state, persist, row, run).await;
    tell(&row.slug, &waiting);
    waiting.is_ok_and(|tasks| !tasks.is_empty())
}

fn tell(fleet: &str, waiting: &Result<Vec<String>, String>) {
    match waiting {
        Ok(tasks) if tasks.is_empty() => {}
        Ok(tasks) => tell_kept_open(fleet, tasks),
        Err(problem) => tell_unread(fleet, problem),
    }
}

fn tell_kept_open(fleet: &str, tasks: &[String]) {
    tracing::info!(%fleet, ?tasks, "fleet: no seat is open, but ready tasks have never had a seat, so the fleet stays open until they are staffed");
}

fn tell_unread(fleet: &str, problem: &str) {
    tracing::warn!(%fleet, %problem, "fleet: mike could not read the fleet to see whether ready tasks still need a seat");
}

async fn unstaffed(
    state: &BaseState,
    persist: &PersistHandle,
    row: &FleetRow,
    run: &str,
) -> Result<Vec<String>, String> {
    let def = FleetDefinition::from_value(&row.definition)?;
    let status = super::status::assemble(state, row).await?;
    let launched: BTreeSet<String> = persist
        .fleet()
        .fleet_seat_rows(run)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|r| r.seat)
        .collect();
    let released = super::release::released(persist, &def, &status).await;
    Ok(never_started(
        &def,
        &released,
        &unstarted_tasks(&status),
        &launched,
    ))
}

#[cfg(test)]
#[path = "unlaunched_tests.rs"]
mod tests;
