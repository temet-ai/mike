use uuid::Uuid;

use super::PersistError;

pub struct StoredReplayRecord {
    pub lane: String,
    pub message_identity: i64,
    pub tail: Vec<u8>,
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn insert_injected_replay_record(
        self,
        machine_uuid: Uuid,
        lane: &str,
        message_identity: i64,
        tail: &[u8],
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("insert_injected_replay_record", async move {
            sqlx::query!(
                "INSERT INTO overrides.injected_replay_records (machine_uuid, lane, message_identity, tail)
                 VALUES ($1, $2, $3, $4)",
                machine_uuid,
                lane,
                message_identity,
                tail,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn live_injected_replay_records(
        self,
        machine_uuid: Uuid,
    ) -> Result<Vec<StoredReplayRecord>, PersistError> {
        crate::metrics::time_db("live_injected_replay_records", async move {
            let rows = sqlx::query_as!(
                StoredReplayRecord,
                "SELECT lane, message_identity, tail
                   FROM overrides.injected_replay_records
                  WHERE machine_uuid = $1
                    AND recorded_at > now() - interval '1 hour'
                  ORDER BY recorded_at, id",
                machine_uuid,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn keep_injected_replay_records(
        self,
        machine_uuid: Uuid,
        lanes: &[String],
    ) -> Result<u64, PersistError> {
        if lanes.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("keep_injected_replay_records", async move {
            let done = sqlx::query!(
                "UPDATE overrides.injected_replay_records
                    SET recorded_at = now()
                  WHERE machine_uuid = $1
                    AND lane = ANY($2)",
                machine_uuid,
                lanes,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn expire_injected_replay_records(
        self,
        machine_uuid: Uuid,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("expire_injected_replay_records", async move {
            let done = sqlx::query!(
                "DELETE FROM overrides.injected_replay_records
                  WHERE machine_uuid = $1
                    AND recorded_at <= now() - interval '1 hour'",
                machine_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }
}
