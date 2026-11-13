use uuid::Uuid;

use super::FleetRow;
use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetCheckoutUser {
    pub fleet_uuid: Uuid,
    pub slug: String,
    pub state: String,
    pub repo: String,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn fleet_checkout_users(self) -> Result<Vec<FleetCheckoutUser>, PersistError> {
        crate::metrics::time_db("launch_fleet_checkout_users", async move {
            Ok(sqlx::query_file_as!(
                FleetCheckoutUser,
                "src/store/persistence/fleet/fleet_checkout_users.sql",
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn pending_fleet_retrospectives(self) -> Result<Vec<FleetRow>, PersistError> {
        crate::metrics::time_db("launch_pending_fleet_retrospectives", async move {
            Ok(sqlx::query_file_as!(
                FleetRow,
                "src/store/persistence/fleet/pending_retrospectives.sql",
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
