use serde_json::Value as JsonValue;

use super::capture_classification::{
    extract_system_blocks_from_body, session_prompt_content_sha256,
};
use super::tools;
use super::{PersistError, PersistHandle};
use crate::store::models::InterceptedRequest;

pub async fn write_system_prompt_and_firing_rules(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    request_db_id: i64,
) -> Result<(), PersistError> {
    let registered_blocks = req
        .system_prompt
        .prompt_sha256
        .as_ref()
        .and(req.system_prompt.blocks.as_ref());
    sqlx::query!(
        "WITH known_prompt AS (
             SELECT sp.id
               FROM system_prompts.system_prompts sp
              WHERE sp.prompt_sha256 = $2::text
              ORDER BY sp.id
              LIMIT 1
         ),
         new_prompt AS (
             INSERT INTO system_prompts.system_prompts
                    (prompt_sha256, blocks, first_seen_at, last_seen_at)
             SELECT $2::text, $3::jsonb, $4, $4
              WHERE $2::text IS NOT NULL
                AND $3::jsonb IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM known_prompt)
             RETURNING id
         ),
         prompt AS (
             SELECT id FROM new_prompt
             UNION ALL
             SELECT id FROM known_prompt
         ),
         new_observation AS (
             INSERT INTO requests.system_prompts
                    (request_id, prompt_sha256, system_prompt_id, override_id,
                     override_applied, unknown_blocks, filter_modified)
             SELECT $1, $2::text, prompt.id, $5, $6, $7, $8
               FROM (VALUES (1)) AS one(n)
               LEFT JOIN prompt ON true
              WHERE NOT EXISTS (
                  SELECT 1 FROM requests.system_prompts WHERE request_id = $1
              )
             RETURNING id
         ),
         observation AS (
             SELECT id FROM new_observation
             UNION ALL
             SELECT rsp.id FROM requests.system_prompts rsp WHERE rsp.request_id = $1
         )
         INSERT INTO requests.system_prompt_firing_rules
                (request_system_prompt_id, filter_rule_id, ord)
         SELECT observation.id, u.rule_id, (u.ord - 1)::int
           FROM observation
          CROSS JOIN UNNEST($9::bigint[]) WITH ORDINALITY AS u(rule_id, ord)
          WHERE NOT EXISTS (
              SELECT 1 FROM requests.system_prompt_firing_rules f
               WHERE f.request_system_prompt_id = observation.id
                 AND f.ord = (u.ord - 1)::int
          )",
        request_db_id,
        req.system_prompt.prompt_sha256.as_deref(),
        registered_blocks,
        req.created_at,
        req.system_prompt.composition_id as Option<i64>,
        req.system_prompt.composition_applied,
        req.system_prompt.unknown_blocks.as_ref(),
        req.system_prompt.filter_modified,
        &req.system_prompt.firing_rule_ids as &[_],
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

pub async fn write_declared_tools_bridge(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    if let Some(body_bytes) = req.request_body.as_ref() {
        let wire_tools = match tools::extract_body_tools(body_bytes) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    request_id = request_db_id,
                    error = %e,
                    "insert_request_initial: tools-bridge extraction skipped"
                );
                Vec::new()
            }
        };
        tools::write_request_tools(&mut *tx, request_db_id, session_db_id, &wire_tools).await?;
    }
    Ok(())
}

impl PersistHandle {
    pub async fn capture_session_prompt_lane(
        &self,
        tx: &mut sqlx::PgConnection,
        req: &InterceptedRequest,
        request_db_id: i64,
        session_db_id: i64,
    ) -> Result<(), PersistError> {
        let request_body_bytes = req.request_body.as_deref();
        let session_prompt_blocks: Option<JsonValue> = req
            .system_prompt
            .blocks
            .clone()
            .or_else(|| request_body_bytes.and_then(extract_system_blocks_from_body));

        let Some(blocks) = session_prompt_blocks else {
            return Ok(());
        };
        let content_sha256 = session_prompt_content_sha256(&blocks);
        let is_main_lane_turn = req.turn_origin == "primary" || req.turn_origin == "primary_first";
        let is_subagent_lane_turn = (req.turn_origin == "subagent"
            || req.turn_origin == "workflow_subagent")
            && req.message_count == 1;

        if is_main_lane_turn {
            self.capture_main_lane_prompt(tx, session_db_id, request_db_id, blocks, content_sha256)
                .await?;
        } else if is_subagent_lane_turn {
            capture_subagent_lane_prompt(
                tx,
                req,
                session_db_id,
                request_db_id,
                blocks,
                content_sha256,
            )
            .await?;
        }
        Ok(())
    }

    async fn capture_main_lane_prompt(
        &self,
        tx: &mut sqlx::PgConnection,
        session_db_id: i64,
        request_db_id: i64,
        blocks: JsonValue,
        content_sha256: Option<String>,
    ) -> Result<(), PersistError> {
        let already_captured = self
            .main_lane_captured
            .read()
            .await
            .contains(&session_db_id);
        if already_captured {
            return Ok(());
        }
        sqlx::query!(
            "INSERT INTO system_prompts.session_prompts
                        (session_id, request_id, lane, blocks, content_sha256)
                     SELECT $1, $2, 'main', $3, $4
                     WHERE NOT EXISTS (
                         SELECT 1 FROM system_prompts.session_prompts
                         WHERE session_id = $1 AND lane = 'main'
                     )",
            session_db_id,
            request_db_id,
            blocks,
            content_sha256,
        )
        .execute(&mut *tx)
        .await?;
        self.main_lane_captured.write().await.insert(session_db_id);
        Ok(())
    }
}

async fn capture_subagent_lane_prompt(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
    blocks: JsonValue,
    content_sha256: Option<String>,
) -> Result<(), PersistError> {
    sqlx::query!(
        "INSERT INTO system_prompts.session_prompts
                    (session_id, request_id, lane, subagent_persona, blocks, content_sha256)
                 SELECT $1, $2, 'subagent', $3, $4, $5
                 WHERE NOT EXISTS (
                     SELECT 1 FROM system_prompts.session_prompts WHERE request_id = $2
                 )",
        session_db_id,
        request_db_id,
        req.subagent_persona.as_deref(),
        blocks,
        content_sha256,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}
