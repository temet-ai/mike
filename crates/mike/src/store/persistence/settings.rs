use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn fetch_settings(&self) -> Result<Vec<(String, serde_json::Value)>, PersistError> {
        crate::metrics::time_db("fetch_settings", async move {
            let rows = sqlx::query!("SELECT key, value FROM config.settings ORDER BY key")
                .fetch_all(&self.pool)
                .await?;

            Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
        })
        .await
    }

    pub async fn upsert_setting(
        &self,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("upsert_setting", async move {
            sqlx::query!(
                r#"INSERT INTO config.settings (key, value)
                 VALUES ($1, $2)
                 ON CONFLICT (key) DO UPDATE
                     SET value      = EXCLUDED.value,
                         updated_at = now()"#,
                key,
                value as &serde_json::Value,
            )
            .execute(&self.pool)
            .await?;

            Ok(())
        })
        .await
    }

    pub async fn upsert_setting_with_source(
        &self,
        key: &str,
        value: &serde_json::Value,
        source_json: &str,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("upsert_setting_with_source", async move {
            let mut tx = self.pool.begin().await?;
            super::change_log::set_change_source(&mut tx, source_json).await?;
            sqlx::query!(
                "INSERT INTO config.settings (key, value)
                 VALUES ($1, $2)
                 ON CONFLICT (key) DO UPDATE
                     SET value      = EXCLUDED.value,
                         updated_at = now()",
                key,
                value as &serde_json::Value,
            )
            .execute(&mut *tx)
            .await?;
            let ids = super::change_log::own_change_ids(&mut tx).await?;
            tx.commit().await?;
            Ok(ids)
        })
        .await
    }

    pub async fn setting_value(
        &self,
        key: &str,
    ) -> Result<Option<serde_json::Value>, PersistError> {
        crate::metrics::time_db("setting_value", async move {
            let value =
                sqlx::query_scalar!("SELECT value FROM config.settings WHERE key = $1", key)
                    .fetch_optional(&self.pool)
                    .await?;
            Ok(value)
        })
        .await
    }
}
