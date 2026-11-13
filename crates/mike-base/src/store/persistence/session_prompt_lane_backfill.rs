use super::{PersistError, PersistHandle};
use serde_json::Value as JsonValue;

pub struct BackfillMainCandidate {
    pub request_id: i64,
    pub session_id: i64,
}

pub struct BackfillSubagentCandidate {
    pub request_id: i64,
    pub session_id: i64,
    pub subagent_persona: Option<String>,
    pub instance_id: Option<i64>,
    pub agent_id: Option<i64>,
}

impl PersistHandle {
    pub async fn backfill_main_lane_candidates(
        &self,
        max_body_bytes: i64,
    ) -> Result<Vec<BackfillMainCandidate>, PersistError> {
        let rows = sqlx::query_as!(
            BackfillMainCandidate,
            r#"SELECT DISTINCT ON (r.session_id)
                      r.id AS request_id, r.session_id
               FROM requests.requests r
               JOIN requests.turn_origins t ON t.id = r.turn_origin_id
               WHERE t.code IN ('primary', 'primary_first')
                 AND r.request_body IS NOT NULL
                 AND octet_length(r.request_body) BETWEEN 1 AND $1::bigint
                 AND NOT EXISTS (
                     SELECT 1 FROM system_prompts.session_prompts sp
                     WHERE sp.session_id = r.session_id AND sp.lane = 'main'
                 )
               ORDER BY r.session_id, r.id ASC"#,
            max_body_bytes,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
    pub async fn backfill_subagent_lane_candidates(
        &self,
        max_body_bytes: i64,
    ) -> Result<Vec<BackfillSubagentCandidate>, PersistError> {
        let rows = sqlx::query_as!(
            BackfillSubagentCandidate,
            r#"SELECT r.id AS request_id, r.session_id, r.subagent_persona,
                      r.agent_instance_id AS instance_id, ai.agent_id AS "agent_id?"
               FROM requests.requests r
               JOIN requests.turn_origins t ON t.id = r.turn_origin_id
               LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
               WHERE t.code IN ('subagent', 'workflow_subagent')
                 AND r.message_count = 1
                 AND r.request_body IS NOT NULL
                 AND octet_length(r.request_body) BETWEEN 1 AND $1::bigint
                 AND NOT EXISTS (
                     SELECT 1 FROM system_prompts.session_prompts sp WHERE sp.request_id = r.id
                 )
               ORDER BY r.id"#,
            max_body_bytes,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
    pub async fn backfill_fetch_request_bodies(
        &self,
        request_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<u8>>, PersistError> {
        if request_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows = sqlx::query!(
            r#"SELECT id, request_body AS "request_body!" FROM requests.requests
                WHERE id = ANY($1) AND request_body IS NOT NULL"#,
            request_ids,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| (r.id, r.request_body)).collect())
    }
    pub async fn backfill_insert_main_lane_row(
        &self,
        session_id: i64,
        request_id: i64,
        blocks: JsonValue,
        content_sha256: Option<String>,
    ) -> Result<bool, PersistError> {
        let result = sqlx::query!(
            "INSERT INTO system_prompts.session_prompts
                (session_id, request_id, lane, blocks, content_sha256)
             SELECT $1, $2, 'main', $3, $4
             WHERE NOT EXISTS (
                 SELECT 1 FROM system_prompts.session_prompts
                 WHERE session_id = $1 AND lane = 'main'
             )",
            session_id,
            request_id,
            blocks,
            content_sha256,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
    pub async fn backfill_insert_subagent_lane_row(
        &self,
        candidate: &BackfillSubagentCandidate,
        blocks: JsonValue,
        content_sha256: Option<String>,
    ) -> Result<bool, PersistError> {
        let result = sqlx::query!(
            "INSERT INTO system_prompts.session_prompts
                (session_id, request_id, lane, subagent_persona, agent_id, instance_id,
                 blocks, content_sha256)
             SELECT $1, $2, 'subagent', $3, $4, $5, $6, $7
             WHERE NOT EXISTS (
                 SELECT 1 FROM system_prompts.session_prompts WHERE request_id = $2
             )",
            candidate.session_id,
            candidate.request_id,
            candidate.subagent_persona,
            candidate.agent_id,
            candidate.instance_id,
            blocks,
            content_sha256,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
