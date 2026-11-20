use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::machine_identity::MachineIdentity;
use crate::state::AppState;
use crate::store::persistence::project_placement::{MachineLocationRoot, UnplacedProject};
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::checkouts::is_scratch;
use crate::workspace::git_repo::{RepoIdentity, repo_identity_bounded};

const TICK: Duration = Duration::from_secs(600);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReconcileReport {
    pub merged: usize,
    pub registered: usize,
}

impl ReconcileReport {
    fn changed(self) -> bool {
        self.merged + self.registered > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    Skip,
    MergeInto(i64),
    RegisterAlone,
}

pub(crate) fn spawn_project_reconcile(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("project_reconcile: no persistence, not spawning");
        return;
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        run(persist, shutdown).await;
    });
}

async fn run(persist: PersistHandle, shutdown: CancellationToken) {
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = interval.tick() => tick(&persist).await,
        }
    }
}

async fn tick(persist: &PersistHandle) {
    let machine = match crate::machine_identity::load_default().await {
        Ok(machine) => machine,
        Err(error) => {
            tracing::warn!(%error, "project_reconcile: no machine identity, skipping this cycle");
            return;
        }
    };
    log_cycle(reconcile_unplaced_projects(persist, &machine).await);
}

fn log_cycle(outcome: Result<ReconcileReport, PersistError>) {
    match outcome {
        Ok(report) if report.changed() => log_changes(report),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "project_reconcile: cycle failed"),
    }
}

fn log_changes(report: ReconcileReport) {
    tracing::info!(
        merged = report.merged,
        registered = report.registered,
        "project_reconcile: cycle finished"
    );
}

pub(crate) async fn reconcile_unplaced_projects(
    persist: &PersistHandle,
    machine: &MachineIdentity,
) -> Result<ReconcileReport, PersistError> {
    let mut report = ReconcileReport::default();
    let mut roots: Option<Vec<MachineLocationRoot>> = None;
    for candidate in persist.unplaced_projects().await? {
        let Some(identity) = own_checkout(&candidate.project_path).await else {
            continue;
        };
        if roots.is_none() {
            roots = Some(persist.machine_location_roots(machine.machine_uuid).await?);
        }
        let known = roots.as_deref().unwrap_or_default();
        match place_candidate(persist, machine, (&candidate, &identity), known).await {
            Ok(Placement::MergeInto(_)) => report.merged += 1,
            Ok(Placement::RegisterAlone) => report.registered += 1,
            Ok(Placement::Skip) => {}
            Err(error) => tracing::warn!(
                %error,
                project_id = candidate.id,
                path = %candidate.project_path,
                "project_reconcile: placing the checkout failed"
            ),
        }
    }
    Ok(report)
}

async fn place_candidate(
    persist: &PersistHandle,
    machine: &MachineIdentity,
    (candidate, identity): (&UnplacedProject, &RepoIdentity),
    roots: &[MachineLocationRoot],
) -> Result<Placement, PersistError> {
    let placement = decide(persist, candidate, identity, roots).await?;
    match placement {
        Placement::Skip => {}
        Placement::RegisterAlone => {
            record_location(persist, machine, identity, candidate.id).await?;
            log_registered(candidate, identity);
        }
        Placement::MergeInto(target) => {
            merge_phantom(persist, machine, (candidate, identity), target).await?;
            log_merged(candidate, identity, target);
        }
    }
    Ok(placement)
}

fn log_registered(candidate: &UnplacedProject, identity: &RepoIdentity) {
    tracing::info!(
        project_id = candidate.id,
        path = %candidate.project_path,
        origin = identity.origin_url.as_deref().unwrap_or(""),
        "project_reconcile: registered the checkout as a location of its own project"
    );
}

fn log_merged(candidate: &UnplacedProject, identity: &RepoIdentity, target: i64) {
    tracing::info!(
        project_id = candidate.id,
        merged_into = target,
        path = %candidate.project_path,
        origin = identity.origin_url.as_deref().unwrap_or(""),
        "project_reconcile: merged an unplaced project into the project of its origin"
    );
}

async fn merge_phantom(
    persist: &PersistHandle,
    machine: &MachineIdentity,
    (candidate, identity): (&UnplacedProject, &RepoIdentity),
    target: i64,
) -> Result<(), PersistError> {
    let uuid = persist.project_uuid_of(target).await?.ok_or_else(|| {
        PersistError::DataInvariant(format!("project {target} to merge into does not exist"))
    })?;
    record_location(persist, machine, identity, target).await?;
    persist
        .web()
        .merge_project_into(&candidate.project_path, uuid)
        .await
        .map_err(|error| {
            PersistError::DataInvariant(format!(
                "merging {} into project {target} failed: {error:?}",
                candidate.project_path
            ))
        })?;
    Ok(())
}

async fn record_location(
    persist: &PersistHandle,
    machine: &MachineIdentity,
    identity: &RepoIdentity,
    project: i64,
) -> Result<(), PersistError> {
    let root = identity.checkout_root.to_string_lossy();
    let location = persist
        .add_checkout_location(machine.machine_uuid, &machine.machine_name, &root, project)
        .await?;
    if let (Some(location), Some(origin)) = (location, identity.origin_url.as_deref()) {
        persist.set_location_origin(location, origin).await?;
    }
    Ok(())
}

async fn own_checkout(project_path: &str) -> Option<RepoIdentity> {
    let trimmed = project_path.trim_end_matches('/');
    if is_scratch(trimmed) {
        return None;
    }
    let dir = Path::new(trimmed);
    if !dir.is_absolute() {
        return None;
    }
    let identity = repo_identity_bounded(dir).await?;
    (identity.checkout_root == dir && identity.main_root == dir).then_some(identity)
}

async fn decide(
    persist: &PersistHandle,
    candidate: &UnplacedProject,
    identity: &RepoIdentity,
    roots: &[MachineLocationRoot],
) -> Result<Placement, PersistError> {
    let root = identity.checkout_root.to_string_lossy();
    let placed_here = roots
        .iter()
        .find(|location| location.root_path.trim_end_matches('/') == root)
        .map(|location| location.project_id)
        .filter(|project| *project != candidate.id);
    if let Some(project) = placed_here {
        return Ok(Placement::MergeInto(project));
    }
    let Some(origin) = identity.origin_url.as_deref() else {
        return Ok(Placement::RegisterAlone);
    };
    let others: Vec<i64> = persist
        .project_ids_for_origin(origin)
        .await?
        .into_iter()
        .filter(|project| *project != candidate.id)
        .collect();
    Ok(match others.as_slice() {
        [] => Placement::RegisterAlone,
        [only] if !candidate.has_sessions => Placement::MergeInto(*only),
        _ => Placement::Skip,
    })
}

#[cfg(test)]
#[path = "project_reconcile_tests.rs"]
mod project_reconcile_tests;
