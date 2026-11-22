use uuid::Uuid;

use super::{FleetHeaderRow, FleetRow};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

struct FleetsInStates(Memo<Vec<FleetRow>>);

impl Default for FleetsInStates {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[RowChangedTable::LaunchFleets],
            per_session: &[],
        }))
    }
}

fn states_key(states: &[String]) -> Uuid {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    states.hash(&mut hasher);
    Uuid::from_u64_pair(hasher.finish(), states.len() as u64)
}

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
        let key = states_key(&states);
        let kept = self.memo::<FleetsInStates>();
        let memo = self
            .feed_memos
            .fed_feed()
            .and(self.feed_memos.bound(&kept.0));
        if let Some(rows) = memo.as_ref().and_then(|memo| memo.get(key)) {
            return Ok(rows);
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let rows = self.read_fleets_in_states(states).await?;
        if let (Some(memo), Some(ticket)) = (memo, ticket) {
            memo.remember(key, ticket, rows.clone());
        }
        Ok(rows)
    }

    async fn read_fleets_in_states(
        self,
        states: Vec<String>,
    ) -> Result<Vec<FleetRow>, PersistError> {
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
