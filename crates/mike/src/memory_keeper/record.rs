use super::supply::SupplyRecord;
use crate::store::persistence::PersistHandle;

pub(crate) fn spawn_record(persist: Option<PersistHandle>, record: SupplyRecord) {
    let Some(persist) = persist else {
        return;
    };
    tokio::spawn(async move {
        if let Err(error) = persist.record_keeper_supply(&record).await {
            tracing::warn!(error = %error, "memory_keeper: supply not recorded");
        }
    });
}

impl PersistHandle {
    pub(crate) async fn record_keeper_supply(
        &self,
        record: &SupplyRecord,
    ) -> Result<(), sqlx::Error> {
        let carrier = record.carrier.as_str();
        let arm = record.arm.as_str();
        crate::metrics::time_db("record_keeper_supply", async move {
            sqlx::query!(
                r#"INSERT INTO memory.keeper_supplies
                       (session_uuid, thread_id, content_hash, carrier, arm, resolution, signals)
                   SELECT $1, $2, $3, $4, $5, $6, $7
                    WHERE NOT EXISTS (
                          SELECT 1 FROM memory.keeper_supplies k
                           WHERE k.session_uuid = $1
                             AND k.thread_id IS NOT DISTINCT FROM $2
                             AND k.content_hash IS NOT DISTINCT FROM $3
                             AND k.resolution = $6)"#,
                record.session,
                record.thread_id,
                record.content_hash.as_deref(),
                carrier,
                arm,
                record.resolution,
                &record.signals,
            )
            .execute(self.pool())
            .await
        })
        .await?;
        Ok(())
    }
}
