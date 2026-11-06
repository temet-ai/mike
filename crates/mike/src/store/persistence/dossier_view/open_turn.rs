use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};
use super::OpenTextRow;

impl PersistHandle {
    pub async fn fetch_open_turn_text(
        &self,
        turn_id: i64,
        at: DateTime<Utc>,
    ) -> Result<Vec<OpenTextRow>, PersistError> {
        crate::metrics::time_db("fetch_open_turn_text", async move {
            let rows = sqlx::query_as!(
                OpenTextRow,
                r#"SELECT b.request_id, b.seq_ord, o.code AS origin,
                          COALESCE(b.content, '') AS "content!"
                     FROM requests.requests r
                     JOIN responses.stream_blocks b ON b.request_id = r.id
                     JOIN responses.stream_block_types bt
                       ON bt.id = b.block_type_id AND bt.code = 'text'
                     JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                    WHERE r.operator_turn_id = $1
                      AND r.created_at <= $2
                      AND o.code NOT LIKE '%sidecar'
                    ORDER BY b.request_id, b.seq_ord"#,
                turn_id,
                at,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
