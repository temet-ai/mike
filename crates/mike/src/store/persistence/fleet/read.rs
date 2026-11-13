use uuid::Uuid;

use super::{FleetHeaderRow, FleetRow};
use crate::store::persistence::PersistError;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn find_fleet(self, key: &str) -> Result<Option<FleetRow>, PersistError> {
        crate::metrics::time_db("launch_find_fleet", async move {
            let row = match Uuid::parse_str(key) {
                Ok(fleet_uuid) => {
                    sqlx::query_file_as!(
                        FleetRow,
                        "src/store/persistence/fleet/find_fleet_by_uuid.sql",
                        fleet_uuid,
                    )
                    .fetch_optional(&self.pool)
                    .await?
                }
                Err(_) => {
                    sqlx::query_file_as!(
                        FleetRow,
                        "src/store/persistence/fleet/find_fleet_by_slug.sql",
                        key,
                    )
                    .fetch_optional(&self.pool)
                    .await?
                }
            };
            Ok(row)
        })
        .await
    }

    pub async fn find_fleet_by_run(self, run_id: &str) -> Result<Option<FleetRow>, PersistError> {
        crate::metrics::time_db("launch_find_fleet_by_run", async move {
            let row = sqlx::query_file_as!(
                FleetRow,
                "src/store/persistence/fleet/find_fleet_by_run.sql",
                run_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn list_fleets_in_states(
        self,
        states: &[&str],
    ) -> Result<Vec<FleetRow>, PersistError> {
        let states: Vec<String> = states.iter().map(|s| (*s).to_owned()).collect();
        crate::metrics::time_db("launch_list_fleets_in_states", async move {
            let rows = sqlx::query_file_as!(
                FleetRow,
                "src/store/persistence/fleet/list_fleets_in_states.sql",
                &states,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_fleet_headers(
        self,
        project_path: Option<&str>,
        limit: i64,
    ) -> Result<Vec<FleetHeaderRow>, PersistError> {
        crate::metrics::time_db("launch_list_fleet_headers", async move {
            let rows = match project_path {
                Some(project_path) => {
                    sqlx::query_file_as!(
                        FleetHeaderRow,
                        "src/store/persistence/fleet/list_project_fleet_headers.sql",
                        project_path,
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        FleetHeaderRow,
                        "src/store/persistence/fleet/list_fleet_headers.sql",
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            Ok(rows)
        })
        .await
    }
}
