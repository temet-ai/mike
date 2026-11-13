use serde_json::Value as JsonValue;

use crate::wire::prompt_blocks::parse_identity_blocks;
use crate::wire::prompt_classify::prompt_sha256_from_block_text;

use super::PersistHandle;
use super::error::PersistError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemPromptBackfillOutcome {
    pub resolved: u64,
    pub unknown_shape: u64,
    pub batches: u64,
}

#[derive(Default)]
struct ShaUpdates {
    request_ids: Vec<i64>,
    shas: Vec<String>,
}

impl ShaUpdates {
    fn push(&mut self, request_id: i64, sha: String) {
        self.request_ids.push(request_id);
        self.shas.push(sha);
    }
}

#[derive(Default)]
struct UnknownShapes {
    request_ids: Vec<i64>,
    blocks: Vec<JsonValue>,
}

impl UnknownShapes {
    fn push(&mut self, request_id: i64, blocks: JsonValue) {
        self.request_ids.push(request_id);
        self.blocks.push(blocks);
    }
}

struct BodyCandidate {
    request_id: i64,
    request_body: Vec<u8>,
}

impl PersistHandle {
    pub async fn backfill_request_system_prompts(
        &self,
        batch_size: i64,
    ) -> Result<SystemPromptBackfillOutcome, PersistError> {
        let mut total = SystemPromptBackfillOutcome::default();
        loop {
            let batch = self
                .backfill_request_system_prompts_batch(batch_size)
                .await?;
            if batch.resolved == 0 && batch.unknown_shape == 0 {
                return Ok(total);
            }
            total.resolved += batch.resolved;
            total.unknown_shape += batch.unknown_shape;
            total.batches += 1;
        }
    }

    pub async fn backfill_request_system_prompts_batch(
        &self,
        batch_size: i64,
    ) -> Result<SystemPromptBackfillOutcome, PersistError> {
        crate::metrics::time_db("backfill_request_system_prompts_batch", async move {
            let mut outcome = SystemPromptBackfillOutcome::default();

            let mut tx = self.pool.begin().await?;
            sqlx::query!("SET LOCAL statement_timeout = 0")
                .execute(&mut *tx)
                .await?;

            let candidates = sqlx::query_as!(
                BodyCandidate,
                r#"SELECT rsp.request_id, r.request_body AS "request_body!"
                   FROM requests.system_prompts rsp
                   JOIN requests.requests r ON r.id = rsp.request_id
                  WHERE rsp.prompt_sha256 IS NULL
                    AND rsp.system_prompt_id IS NULL
                    AND rsp.unknown_blocks IS NULL
                    AND r.request_body IS NOT NULL
                  ORDER BY rsp.request_id ASC
                  LIMIT $1"#,
                batch_size,
            )
            .fetch_all(&mut *tx)
            .await?;

            if candidates.is_empty() {
                return Ok(outcome);
            }

            let mut plain = ShaUpdates::default();
            let mut unknown = UnknownShapes::default();
            for BodyCandidate {
                request_id,
                request_body: body_bytes,
            } in candidates
            {
                let parsed: Result<JsonValue, _> = serde_json::from_slice(&body_bytes);
                let Ok(body) = parsed else {
                    unknown.push(request_id, serde_json::json!({"reason": "missing"}));
                    outcome.unknown_shape += 1;
                    continue;
                };

                match parse_identity_blocks(&body) {
                    Ok(identity) => {
                        let sha = prompt_sha256_from_block_text(&identity.role_body_text);
                        let blocks = body.get("system").cloned().filter(JsonValue::is_array);
                        let Some(blocks) = blocks else {
                            plain.push(request_id, sha);
                            outcome.resolved += 1;
                            continue;
                        };

                        Self::link_resolved_prompt(&mut tx, request_id, &sha, &blocks).await?;
                        outcome.resolved += 1;
                    }
                    Err(reason) => {
                        unknown.push(request_id, reason.to_json());
                        outcome.unknown_shape += 1;
                    }
                }
            }
            Self::apply_plain_shas(&mut tx, &plain).await?;
            Self::record_unknown_shapes(&mut tx, &unknown).await?;
            tx.commit().await?;
            Ok(outcome)
        })
        .await
    }

    pub async fn reclassify_unknown_shape_system_prompts(
        &self,
        batch_size: i64,
    ) -> Result<SystemPromptBackfillOutcome, PersistError> {
        let mut total = SystemPromptBackfillOutcome::default();
        let mut cursor: i64 = 0;
        loop {
            let (batch, next) = self
                .reclassify_unknown_shape_batch(batch_size, cursor)
                .await?;
            let Some(next) = next else {
                return Ok(total);
            };
            total.resolved += batch.resolved;
            total.unknown_shape += batch.unknown_shape;
            total.batches += 1;
            cursor = next;
        }
    }

    async fn reclassify_unknown_shape_batch(
        &self,
        batch_size: i64,
        after_request_id: i64,
    ) -> Result<(SystemPromptBackfillOutcome, Option<i64>), PersistError> {
        crate::metrics::time_db("reclassify_unknown_shape_batch", async move {
            let mut outcome = SystemPromptBackfillOutcome::default();
            let mut tx = self.pool.begin().await?;
            sqlx::query!("SET LOCAL statement_timeout = 0")
                .execute(&mut *tx)
                .await?;

            let candidates = sqlx::query_as!(
                BodyCandidate,
                r#"SELECT rsp.request_id, r.request_body AS "request_body!"
                   FROM requests.system_prompts rsp
                   JOIN requests.requests r ON r.id = rsp.request_id
                  WHERE rsp.request_id > $2
                    AND rsp.prompt_sha256 IS NULL
                    AND rsp.system_prompt_id IS NULL
                    AND rsp.unknown_blocks->>'reason' IN ('non_3_block', 'string_form')
                    AND r.request_body IS NOT NULL
                  ORDER BY rsp.request_id ASC
                  LIMIT $1"#,
                batch_size,
                after_request_id,
            )
            .fetch_all(&mut *tx)
            .await?;

            let Some(highest) = candidates.last().map(|row| row.request_id) else {
                return Ok((outcome, None));
            };

            let mut plain = ShaUpdates::default();
            for BodyCandidate {
                request_id,
                request_body: body_bytes,
            } in candidates
            {
                let Ok(body) = serde_json::from_slice::<JsonValue>(&body_bytes) else {
                    outcome.unknown_shape += 1;
                    continue;
                };
                let Ok(identity) = parse_identity_blocks(&body) else {
                    outcome.unknown_shape += 1;
                    continue;
                };
                let sha = prompt_sha256_from_block_text(&identity.role_body_text);
                let blocks = body.get("system").cloned().filter(JsonValue::is_array);

                if let Some(blocks) = blocks {
                    Self::link_reclassified_prompt(&mut tx, request_id, &sha, &blocks).await?;
                } else {
                    plain.push(request_id, sha);
                }
                outcome.resolved += 1;
            }

            Self::apply_plain_shas_clearing_unknown(&mut tx, &plain).await?;
            tx.commit().await?;
            Ok((outcome, Some(highest)))
        })
        .await
    }

    async fn link_resolved_prompt(
        tx: &mut sqlx::PgConnection,
        request_id: i64,
        sha: &str,
        blocks: &JsonValue,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "-- grader-allow: each iteration reads the system_prompts row the previous iteration inserted (shared prompt deduplicated within the batch)
             WITH known_prompt AS (
                 SELECT sp.id
                   FROM system_prompts.system_prompts sp
                  WHERE sp.prompt_sha256 = $2
                  ORDER BY sp.id
                  LIMIT 1
             ),
             new_prompt AS (
                 INSERT INTO system_prompts.system_prompts
                        (prompt_sha256, blocks, first_seen_at, last_seen_at)
                 SELECT $2, $3, r.created_at, r.created_at
                   FROM requests.requests r
                  WHERE r.id = $1
                    AND NOT EXISTS (SELECT 1 FROM known_prompt)
                 RETURNING id
             ),
             prompt AS (
                 SELECT id FROM new_prompt
                 UNION ALL
                 SELECT id FROM known_prompt
             )
             UPDATE requests.system_prompts rsp
                SET prompt_sha256 = $2,
                    system_prompt_id = prompt.id
               FROM prompt
              WHERE rsp.request_id = $1
                AND rsp.prompt_sha256 IS NULL
                AND rsp.system_prompt_id IS NULL",
            request_id,
            sha,
            blocks,
        )
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn link_reclassified_prompt(
        tx: &mut sqlx::PgConnection,
        request_id: i64,
        sha: &str,
        blocks: &JsonValue,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "-- grader-allow: each iteration reads the system_prompts row the previous iteration inserted (shared prompt deduplicated within the batch)
             WITH known_prompt AS (
                 SELECT sp.id
                   FROM system_prompts.system_prompts sp
                  WHERE sp.prompt_sha256 = $2
                  ORDER BY sp.id
                  LIMIT 1
             ),
             new_prompt AS (
                 INSERT INTO system_prompts.system_prompts
                        (prompt_sha256, blocks, first_seen_at, last_seen_at)
                 SELECT $2, $3, r.created_at, r.created_at
                   FROM requests.requests r
                  WHERE r.id = $1
                    AND NOT EXISTS (SELECT 1 FROM known_prompt)
                 RETURNING id
             ),
             prompt AS (
                 SELECT id FROM new_prompt
                 UNION ALL
                 SELECT id FROM known_prompt
             )
             UPDATE requests.system_prompts rsp
                SET prompt_sha256 = $2,
                    system_prompt_id = prompt.id,
                    unknown_blocks = NULL
               FROM prompt
              WHERE rsp.request_id = $1
                AND rsp.prompt_sha256 IS NULL",
            request_id,
            sha,
            blocks,
        )
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn apply_plain_shas(
        tx: &mut sqlx::PgConnection,
        updates: &ShaUpdates,
    ) -> Result<(), PersistError> {
        if updates.request_ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            "UPDATE requests.system_prompts rsp
                SET prompt_sha256 = v.sha
               FROM unnest($1::bigint[], $2::text[]) AS v(request_id, sha)
              WHERE rsp.request_id = v.request_id
                AND rsp.prompt_sha256 IS NULL",
            &updates.request_ids,
            &updates.shas,
        )
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn apply_plain_shas_clearing_unknown(
        tx: &mut sqlx::PgConnection,
        updates: &ShaUpdates,
    ) -> Result<(), PersistError> {
        if updates.request_ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            "UPDATE requests.system_prompts rsp
                SET prompt_sha256 = v.sha,
                    unknown_blocks = NULL
               FROM unnest($1::bigint[], $2::text[]) AS v(request_id, sha)
              WHERE rsp.request_id = v.request_id
                AND rsp.prompt_sha256 IS NULL",
            &updates.request_ids,
            &updates.shas,
        )
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn record_unknown_shapes(
        tx: &mut sqlx::PgConnection,
        unknown: &UnknownShapes,
    ) -> Result<(), PersistError> {
        if unknown.request_ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            "UPDATE requests.system_prompts rsp
                SET unknown_blocks = v.unknown_blocks
               FROM unnest($1::bigint[], $2::jsonb[]) AS v(request_id, unknown_blocks)
              WHERE rsp.request_id = v.request_id
                AND rsp.prompt_sha256 IS NULL
                AND rsp.system_prompt_id IS NULL
                AND rsp.unknown_blocks IS NULL",
            &unknown.request_ids,
            &unknown.blocks,
        )
        .execute(tx)
        .await?;
        Ok(())
    }

    pub async fn count_system_prompts_missing_and_uncompactable(
        &self,
    ) -> Result<i64, PersistError> {
        let count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\"
               FROM requests.system_prompts rsp
               JOIN requests.requests r ON r.id = rsp.request_id
              WHERE rsp.prompt_sha256 IS NULL
                AND rsp.system_prompt_id IS NULL
                AND r.request_body IS NULL"
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }
}

#[cfg(test)]
pub mod tests;
