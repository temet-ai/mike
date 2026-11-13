use chrono::{DateTime, Utc};

use super::super::PersistError;

#[derive(Debug, Clone)]
pub struct FleetEdgeRow {
    pub from_session_id: i64,
    pub to_session_id: i64,
    pub first_ts: DateTime<Utc>,
    pub last_ts: DateTime<Utc>,
    pub count: i64,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn fetch_fleet_message_edges(
        self,
        session_ids: &[i64],
    ) -> Result<Vec<FleetEdgeRow>, PersistError> {
        crate::metrics::time_db("fleet_fetch_message_edges", async move {
            let rows = sqlx::query!(
                r#"SELECT
                       m.from_session_id AS "from_session_id!",
                       m.to_session_id   AS "to_session_id!",
                       MIN(m.queued_at)  AS "first_ts!",
                       MAX(m.queued_at)  AS "last_ts!",
                       COUNT(*)          AS "edge_count!"
                   FROM mailbox.messages m
                  WHERE m.from_session_id = ANY($1) AND m.to_session_id = ANY($1)
                  GROUP BY m.from_session_id, m.to_session_id"#,
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(rows
                .into_iter()
                .map(|r| FleetEdgeRow {
                    from_session_id: r.from_session_id,
                    to_session_id: r.to_session_id,
                    first_ts: r.first_ts,
                    last_ts: r.last_ts,
                    count: r.edge_count,
                })
                .collect())
        })
        .await
    }
}
