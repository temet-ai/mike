use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct UnsettledFleetLaunch {
    pub id: i64,
    pub session_uuid: Option<Uuid>,
    pub backend: String,
    pub seat: Option<String>,
    pub surface_ref: Option<String>,
    pub pid: Option<i32>,
    pub opened: bool,
    pub result: String,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_process_exited: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PendingFleetCommand {
    pub id: i64,
    pub run_id: String,
    pub command: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FleetRunAdmission {
    pub fleet_uuid: Uuid,
    pub state: String,
    pub run_id: String,
}

impl FleetRunAdmission {
    pub fn admits_work(&self) -> bool {
        matches!(self.state.as_str(), "starting" | "running")
    }
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn unsettled_fleet_launches(
        self,
        run_id: &str,
    ) -> Result<Vec<UnsettledFleetLaunch>, PersistError> {
        crate::metrics::time_db("launch_unsettled_fleet_launches", async move {
            Ok(sqlx::query_file_as!(
                UnsettledFleetLaunch,
                "src/store/persistence/fleet/lifecycle_sql/unsettled_launches.sql",
                run_id,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn pending_fleet_commands(
        self,
        run_id: &str,
    ) -> Result<Vec<PendingFleetCommand>, PersistError> {
        crate::metrics::time_db("launch_pending_fleet_commands", async move {
            Ok(sqlx::query_file_as!(
                PendingFleetCommand,
                "src/store/persistence/fleet/lifecycle_sql/pending_commands.sql",
                run_id,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fleet_run_admission(
        self,
        run_id: &str,
    ) -> Result<Option<FleetRunAdmission>, PersistError> {
        crate::metrics::time_db("launch_fleet_run_admission", async move {
            let mut rows = sqlx::query_file_as!(
                FleetRunAdmission,
                "src/store/persistence/fleet/lifecycle_sql/run_admission.sql",
                run_id,
            )
            .fetch_all(&self.pool)
            .await?;
            if rows.len() > 1 {
                return Err(PersistError::DataInvariant(format!(
                    "run {run_id:?} belongs to multiple fleets; work admission is ambiguous"
                )));
            }
            Ok(rows.pop())
        })
        .await
    }

    pub async fn record_fleet_launch_verified_exit(
        self,
        launch_id: i64,
        run_id: &str,
        reason: &str,
    ) -> Result<(), PersistError> {
        self.launch()
            .record_fleet_launch_closure(launch_id, run_id, reason, Some(true))
            .await
    }
}
