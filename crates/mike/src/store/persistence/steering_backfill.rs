use super::PersistError;
use super::steering;

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub(crate) async fn steering_backfill_sent_request_id(
        self,
        intervention_uuid: &str,
        request_id: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("steering_backfill_sent_request_id", async {
            let mut conn = self.pool.acquire().await?;
            steering::backfill_sent_request_id(&mut conn, intervention_uuid, request_id).await
        })
        .await
    }
    pub(crate) async fn steering_backfill_trigger_request_id(
        self,
        intervention_uuid: &str,
        request_id: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("steering_backfill_trigger_request_id", async {
            let mut conn = self.pool.acquire().await?;
            steering::backfill_trigger_request_id(&mut conn, intervention_uuid, request_id).await
        })
        .await
    }
    pub(crate) async fn steering_backfill_delivered_request_id(
        self,
        intervention_uuid: &str,
        request_id: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("steering_backfill_delivered_request_id", async {
            let mut conn = self.pool.acquire().await?;
            steering::backfill_delivered_request_id(&mut conn, intervention_uuid, request_id).await
        })
        .await
    }
}
