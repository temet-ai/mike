use super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn fetch_feature_toggles(&self) -> Result<Vec<(String, bool, bool)>, PersistError> {
        crate::metrics::time_db("fetch_feature_toggles", async move {
            let rows = sqlx::query!(
                "SELECT key, enabled, operator_set FROM config.feature_toggles ORDER BY key"
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(rows
                .into_iter()
                .map(|r| (r.key, r.enabled, r.operator_set))
                .collect())
        })
        .await
    }

    pub async fn upsert_feature_toggle(
        &self,
        key: &str,
        enabled: bool,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("upsert_feature_toggle", async move {
            sqlx::query!(
                "INSERT INTO config.feature_toggles (key, enabled, operator_set)
                 VALUES ($1, $2, true)
                 ON CONFLICT (key) DO UPDATE
                     SET enabled      = EXCLUDED.enabled,
                         operator_set = true,
                         updated_at   = now()",
                key,
                enabled,
            )
            .execute(&self.pool)
            .await?;

            Ok(())
        })
        .await
    }

    pub async fn upsert_feature_toggle_with_source(
        &self,
        key: &str,
        enabled: bool,
        source_json: &str,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("upsert_feature_toggle_with_source", async move {
            let mut tx = self.pool.begin().await?;
            super::change_log::set_change_source(&mut tx, source_json).await?;
            sqlx::query!(
                "INSERT INTO config.feature_toggles (key, enabled, operator_set)
                 VALUES ($1, $2, true)
                 ON CONFLICT (key) DO UPDATE
                     SET enabled      = EXCLUDED.enabled,
                         operator_set = true,
                         updated_at   = now()",
                key,
                enabled,
            )
            .execute(&mut *tx)
            .await?;
            let ids = super::change_log::own_change_ids(&mut tx).await?;
            tx.commit().await?;
            Ok(ids)
        })
        .await
    }
}
