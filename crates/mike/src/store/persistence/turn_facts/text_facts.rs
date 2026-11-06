use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct TextFacts {
    pub turn_id: i64,
    pub text_chars: i32,
    pub text_cut_at: i32,
    pub last_text_origin: Option<String>,
    pub text_spans: serde_json::Value,
    pub consistent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopFacts {
    pub turn_id: i64,
    pub stop_request_id: Option<i64>,
    pub stop_reason: Option<String>,
    pub interrupted: bool,
    pub close_cause: String,
}

impl PersistHandle {
    pub async fn load_text_facts(&self, turn_ids: &[i64]) -> Result<Vec<TextFacts>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("load_turn_text_facts", async move {
            Ok(sqlx::query_file_as!(
                TextFacts,
                "src/store/persistence/turn_facts/text_facts.sql",
                turn_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn refresh_final_assistant_text(
        &self,
        turn_ids: &[i64],
    ) -> Result<u64, PersistError> {
        if turn_ids.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("refresh_final_assistant_text", async move {
            let done = sqlx::query!(
                r"UPDATE memory.operator_turns t
                     SET final_assistant_text = s.text
                    FROM (
                        SELECT r.operator_turn_id AS turn_id,
                               string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS text
                          FROM requests.requests r
                          JOIN responses.stream_blocks b ON b.request_id = r.id
                          JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                          JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
                         WHERE r.operator_turn_id = ANY($1)
                           AND bt.code = 'text'
                           AND o.code NOT LIKE '%sidecar'
                         GROUP BY r.operator_turn_id
                    ) s
                   WHERE t.id = s.turn_id
                     AND t.final_assistant_text IS DISTINCT FROM s.text",
                turn_ids,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn load_stop_facts(&self, turn_ids: &[i64]) -> Result<Vec<StopFacts>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("load_turn_stop_facts", async move {
            Ok(sqlx::query_file_as!(
                StopFacts,
                "src/store/persistence/turn_facts/stop_facts.sql",
                turn_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
