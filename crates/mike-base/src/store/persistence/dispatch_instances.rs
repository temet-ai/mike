use super::id_resolvers::resolve_session_id;
use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn create_instances_for_dispatches(
        &self,
        session_id: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("create_instances_for_dispatches", async {
            let session_db_id: i64 = self
                .cached_session_or_ensure(
                    session_id,
                    resolve_session_id(&self.pool, session_id, None, None),
                )
                .await?;

            sqlx::query_file!(
                "src/store/persistence/dispatch_instances/create_for_session.sql",
                session_db_id,
            )
            .fetch_one(&self.pool)
            .await?;

            Ok(())
        })
        .await
    }
}
