use crate::launch::LaunchError;
use crate::launch::fleet::owned_execution::{self, RunFence};
use crate::launch::seat::{SeatAdmission, SeatFuture};
use crate::store::persistence::fleet::FleetLaunchAdmission;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch::LaunchSurfaceRefs;

use mike_launch::launch::cli::ledger::Ledger;

pub struct Admission {
    durable: FleetLaunchAdmission,
    _local: RunFence,
}

pub async fn admit(ledger: &Ledger, run: &str) -> Result<Option<Admission>, LaunchError> {
    let persist = match ledger.fleet_database() {
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

impl SeatAdmission for Admission {
    fn record_opened<'a>(
        self: Box<Self>,
        launch: i64,
        refs: &'a LaunchSurfaceRefs,
    ) -> SeatFuture<'a> {
        Box::pin(async move { self.record(launch, refs).await })
    }
}

impl Admission {
    async fn record(self, launch: i64, refs: &LaunchSurfaceRefs) -> Result<(), LaunchError> {
        self.durable.record_opened(launch, refs).await.map_err(|error| {
            LaunchError::Failed(format!(
                "the backend opened the seat, but its durable launch receipt could not be saved; the pending launch remains unresolved: {error}"
            ))
        })
    }
}
