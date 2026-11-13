use uuid::Uuid;

use super::PersistError;

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn link_channel_from_request_body(
        self,
        cse_id: &str,
        session_uuid: Uuid,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("remote_control_link_channel_from_request_body", async {
            sqlx::query_file_scalar!(
                "src/store/persistence/remote_control/link_channel_from_request_body.sql",
                cse_id,
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}
