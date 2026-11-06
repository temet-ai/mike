use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{FleetRow, NewFleet};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn ensure_fleet_comms_thread(
        &self,
        fleet_uuid: Uuid,
    ) -> Result<Option<Uuid>, PersistError> {
        crate::metrics::time_db("launch_ensure_fleet_comms_thread", async move {
            let thread = sqlx::query_scalar!(
                r#"SELECT mailbox.ensure_fleet_thread($1) AS "thread_uuid?""#,
                fleet_uuid,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(thread)
        })
        .await
    }

    pub async fn insert_fleet(&self, new: &NewFleet) -> Result<FleetRow, PersistError> {
        crate::metrics::time_db("launch_insert_fleet", async move {
            let row = sqlx::query_file_as!(
                FleetRow,
                "src/store/persistence/fleet/insert_fleet.sql",
                new.slug,
                new.project_path,
                new.definition,
                new.missing,
                new.created_by,
            )
            .fetch_one(&self.pool)
            .await?;
            self.fleets_changed();
            Ok(row)
        })
        .await
    }

    pub async fn save_fleet_definition(
        &self,
        fleet_uuid: Uuid,
        definition: &serde_json::Value,
        missing: &serde_json::Value,
        seen_updated_at: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("launch_save_fleet_definition", async move {
            let updated_at = sqlx::query_file_scalar!(
                "src/store/persistence/fleet/save_fleet_definition.sql",
                fleet_uuid,
                definition,
                missing,
                seen_updated_at,
            )
            .fetch_optional(&self.pool)
            .await?;
            self.fleets_changed();
            Ok(updated_at)
        })
        .await
    }

    pub async fn set_fleet_state(
        &self,
        fleet_uuid: Uuid,
        state: &str,
        run_id: Option<&str>,
        start_report: Option<&serde_json::Value>,
    ) -> Result<FleetRow, PersistError> {
        crate::metrics::time_db("launch_set_fleet_state", async move {
            self.set_fleet_state_guarded(fleet_uuid, state, run_id, start_report)
                .await
        })
        .await
    }
}
