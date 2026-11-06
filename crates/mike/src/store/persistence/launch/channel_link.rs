use uuid::Uuid;

use super::LinkOutcome;
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn link_channel_to_session(
        &self,
        cse_id: &str,
        session_uuid: Uuid,
        cwd: &str,
    ) -> Result<LinkOutcome, PersistError> {
        crate::metrics::time_db("launch_link_channel_to_session", async move {
            let linked = sqlx::query_file_scalar!(
                "src/store/persistence/launch/link_channel_to_session.sql",
                cse_id,
                session_uuid,
                cwd,
            )
            .fetch_optional(&self.pool)
            .await?;
            if linked.is_some() {
                return Ok(LinkOutcome::Linked);
            }
            let exists = sqlx::query_file_scalar!(
                "src/store/persistence/launch/channel_exists.sql",
                cse_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(if exists {
                LinkOutcome::AlreadyLinked
            } else {
                LinkOutcome::NoChannel
            })
        })
        .await
    }
}
