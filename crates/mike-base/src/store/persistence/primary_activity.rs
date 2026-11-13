use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn has_recent_primary_activity(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (
                SELECT 1 FROM requests.requests r
                JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                WHERE r.created_at >= $1 AND o.code IN ('primary', 'primary_first')
            ) AS "active!""#,
            since
        )
        .fetch_one(&self.pool)
        .await?)
    }
}
