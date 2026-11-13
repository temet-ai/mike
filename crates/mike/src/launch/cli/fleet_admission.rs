use crate::launch::fleet::owned_execution::{self, RunFence};
use crate::launch::{LaunchError, OpenRequest, Surface};
use crate::store::persistence::fleet::FleetLaunchAdmission;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch::LaunchSurfaceRefs;

use super::super::runner::Runner;

pub(super) struct Admission {
    durable: FleetLaunchAdmission,
    _local: RunFence,
}

pub(super) async fn admit(
    runner: &Runner<'_>,
    run: Option<&str>,
) -> Result<Option<Admission>, LaunchError> {
    let Some(run) = run else {
        return Ok(None);
    };
    let persist = match runner.ledger.fleet_database() {
        Ok(persist) => persist,
        Err(problem) if run.starts_with("fleet-") => {
            return Err(LaunchError::Refused(format!(
                "fleet launch needs a direct durable run fence: {problem}; a fleet seat has no database access, so start the seat through the proxy with the spawn_session MCP tool (run, seat, role, claude_args), which holds the fence"
            )));
        }
        Err(_) => return Ok(None),
    };
    let local = owned_execution::registry().fence(run).await;
    if local.is_stopping() {
        return Err(LaunchError::Refused(format!("fleet run {run} is stopping")));
    }
    let durable = persist
        .fleet()
        .begin_fleet_launch(run)
        .await
        .map_err(|error| LaunchError::Refused(error.to_string()))?;
    match durable {
        Some(durable) => Ok(Some(Admission {
            durable,
            _local: local,
        })),
        None if run.starts_with("fleet-") => Err(LaunchError::Refused(format!(
            "no current fleet owns exact run {run}; no seat was launched"
        ))),
        None => Ok(None),
    }
}

impl Admission {
    pub(super) async fn record_opened(
        self,
        launch: i64,
        refs: &LaunchSurfaceRefs,
    ) -> Result<(), LaunchError> {
        self.durable.record_opened(launch, refs).await.map_err(|error| {
            LaunchError::Failed(format!(
                "the backend opened the seat, but its durable launch receipt could not be saved; the pending launch remains unresolved: {error}"
            ))
        })
    }
}

pub(super) async fn record_opened(
    runner: &Runner<'_>,
    admission: Option<Admission>,
    launch: Option<i64>,
    request: &OpenRequest,
    surface: &Surface,
) -> Result<(), LaunchError> {
    let refs = super::super::open_outcome::surface_refs(surface);
    if let Some(admission) = admission {
        let launch = launch.ok_or_else(|| {
            LaunchError::Failed("fleet opened without a launch reservation".into())
        })?;
        admission.record_opened(launch, &refs).await?;
    } else {
        runner.ledger.record_opened(launch, &refs).await;
        super::super::run_seat::record_workspace(runner, request, surface).await;
    }
    Ok(())
}

pub(super) async fn record_unconfirmed(
    runner: &Runner<'_>,
    launch: Option<i64>,
    error: &LaunchError,
) {
    let mut outcome = super::super::open_outcome::failed_outcome("fleet_open_unconfirmed", error);
    outcome.result = "pending".into();
    runner.ledger.record_outcome(launch, &outcome).await;
}
