use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::store::persistence::PersistError;

pub struct FleetStateLock {
    pub state: String,
    pub run_id: Option<String>,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn begin_fleet_spawn_fence(
        self,
        fleet_uuid: Uuid,
    ) -> Result<(Transaction<'static, Postgres>, Option<FleetStateLock>), PersistError> {
        crate::metrics::time_db("begin_fleet_spawn_fence", async move {
            let mut transaction = self.pool.begin().await?;
            let lock = sqlx::query_as!(
                FleetStateLock,
                "SELECT state, run_id FROM launch.fleets WHERE fleet_uuid = $1 FOR KEY SHARE",
                fleet_uuid,
            )
            .fetch_optional(&mut *transaction)
            .await?;
            Ok((transaction, lock))
        })
        .await
    }
}
