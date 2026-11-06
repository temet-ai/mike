use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrphanedMessageStats {
    pub count: i64,
    pub total_bytes: i64,
}

impl PersistHandle {
    pub async fn sweep_orphaned_messages(&self, batch_size: i64) -> Result<u64, PersistError> {
        crate::metrics::time_db("sweep_orphaned_messages", async move {
            let r = sqlx::query!(
                "WITH candidates AS (
                     SELECT id FROM messages.messages
                     ORDER BY last_seen ASC
                     LIMIT $1
                 )
                 DELETE FROM messages.messages m
                 USING candidates c
                 WHERE m.id = c.id
                   AND NOT EXISTS (
                       SELECT 1 FROM requests.messages rm WHERE rm.message_id = m.id
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM requests.deltas rd WHERE rd.message_id = m.id
                   )",
                batch_size,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected())
        })
        .await
    }

    pub async fn count_orphaned_messages(&self) -> Result<OrphanedMessageStats, PersistError> {
        crate::metrics::time_db("count_orphaned_messages", async move {
            let row = sqlx::query!(
                r#"SELECT count(*) AS "count!", COALESCE(sum(byte_size), 0)::bigint AS "total_bytes!"
                   FROM messages.messages m
                   WHERE NOT EXISTS (
                       SELECT 1 FROM requests.messages rm WHERE rm.message_id = m.id
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM requests.deltas rd WHERE rd.message_id = m.id
                   )"#
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(OrphanedMessageStats {
                count: row.count,
                total_bytes: row.total_bytes,
            })
        })
        .await
    }
}
