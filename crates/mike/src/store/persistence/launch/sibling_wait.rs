use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn record_sibling_wait(
        &self,
        session: Uuid,
        seconds: i32,
        waited_for: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_record_sibling_wait", async move {
            let hit = sqlx::query_file_scalar!(
                "src/store/persistence/launch/record_sibling_wait.sql",
                session,
                seconds,
                waited_for,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(hit.is_some())
        })
        .await
    }
}
