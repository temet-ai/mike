use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn change_place(&self, session: i64, checkout: &str) -> Result<i64, PersistError> {
        crate::metrics::time_db("change_record_place", async move {
            let place = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/change_place.sql",
                session,
                checkout,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(place.unwrap_or(0))
        })
        .await
    }

    pub async fn move_change_place(
        &self,
        session: i64,
        checkout: &str,
        to: i64,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("change_record_move_place", async move {
            sqlx::query_file!(
                "src/store/persistence/change_record/move_change_place.sql",
                session,
                checkout,
                to,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}
