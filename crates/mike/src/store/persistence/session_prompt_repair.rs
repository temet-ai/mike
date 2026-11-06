use super::PersistHandle;
use super::error::PersistError;
use super::session_prompt_content_sha256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SessionPromptRepairOutcome {
    pub(crate) repaired: u64,
    pub(crate) uncorroborated: u64,
    pub(crate) no_blocks: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct MisattributedMainRow {
    pub(crate) ledger_id: i64,
    pub(crate) session_id: i64,
    pub(crate) current_request_id: i64,
    pub(crate) winner_request_id: i64,
    pub(crate) winner_system_prompt_id: i64,
    pub(crate) winner_occurrences: i64,
}

impl PersistHandle {
    pub(crate) async fn find_misattributed_main_lane_rows(
        &self,
    ) -> Result<Vec<MisattributedMainRow>, PersistError> {
        crate::metrics::time_db("find_misattributed_main_lane_rows", async {
            let rows = sqlx::query!(
                r#"
                WITH occ AS (
                    SELECT r.session_id,
                           x.system_prompt_id,
                           count(*)  AS occurrences,
                           min(r.id) AS first_request_id
                      FROM requests.system_prompts x
                      JOIN requests.requests r        ON r.id = x.request_id
                      JOIN requests.turn_origins o    ON o.id = r.turn_origin_id
                      JOIN system_prompts.session_prompts sp
                           ON sp.session_id = r.session_id AND sp.lane = 'main'
                     WHERE x.system_prompt_id IS NOT NULL
                       AND o.code IN ('primary', 'primary_first')
                     GROUP BY r.session_id, x.system_prompt_id
                ),
                winner AS (
                    SELECT DISTINCT ON (session_id)
                           session_id, system_prompt_id, occurrences, first_request_id
                      FROM occ
                     ORDER BY session_id, occurrences DESC, first_request_id ASC
                )
                SELECT sp.id            AS "ledger_id!",
                       sp.session_id    AS "session_id!",
                       sp.request_id    AS "current_request_id!",
                       w.first_request_id AS "winner_request_id!",
                       w.system_prompt_id AS "winner_system_prompt_id!",
                       w.occurrences    AS "winner_occurrences!"
                  FROM system_prompts.session_prompts sp
                  JOIN winner w ON w.session_id = sp.session_id
                 WHERE sp.lane = 'main'
                   AND sp.request_id <> w.first_request_id
                 ORDER BY sp.session_id
                "#
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(rows
                .into_iter()
                .map(|r| MisattributedMainRow {
                    ledger_id: r.ledger_id,
                    session_id: r.session_id,
                    current_request_id: r.current_request_id,
                    winner_request_id: r.winner_request_id,
                    winner_system_prompt_id: r.winner_system_prompt_id,
                    winner_occurrences: r.winner_occurrences,
                })
                .collect())
        })
        .await
    }

    pub(crate) async fn repair_main_lane_session_prompts(
        &self,
        dry_run: bool,
    ) -> Result<SessionPromptRepairOutcome, PersistError> {
        let candidates = self.find_misattributed_main_lane_rows().await?;
        let mut outcome = SessionPromptRepairOutcome::default();

        for candidate in candidates {
            if candidate.winner_occurrences < 2 {
                outcome.uncorroborated += 1;
                continue;
            }

            let blocks = sqlx::query_scalar!(
                "SELECT blocks FROM system_prompts.system_prompts WHERE id = $1",
                candidate.winner_system_prompt_id,
            )
            .fetch_optional(&self.pool)
            .await?;

            let Some(blocks) = blocks else {
                outcome.no_blocks += 1;
                continue;
            };

            let content_sha256 = session_prompt_content_sha256(&blocks);

            if dry_run {
                outcome.repaired += 1;
                continue;
            }

            sqlx::query!(
                "UPDATE system_prompts.session_prompts
                    SET request_id = $2,
                        blocks = $3,
                        content_sha256 = $4
                  WHERE id = $1
                    AND lane = 'main'",
                candidate.ledger_id,
                candidate.winner_request_id,
                blocks,
                content_sha256,
            )
            .execute(&self.pool)
            .await?;
            outcome.repaired += 1;
        }

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose_prompt::classify::prompt_sha256_from_block_text;
    use crate::store::persistence::test_support;
    use serde_json::json;

    async fn seed_session(pool: &sqlx::PgPool) -> anyhow::Result<i64> {
        let project_path = format!("/tmp/session-prompt-repair-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now();
        let project_id: i64 = sqlx::query_scalar(
            "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, $2, $2) RETURNING id",
        )
        .bind(&project_path)
        .bind(now)
        .fetch_one(pool)
        .await?;
        let session_id: i64 = sqlx::query_scalar(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1::text::uuid, $2, $3, $3) RETURNING id",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(project_id)
        .bind(now)
        .fetch_one(pool)
        .await?;
        Ok(session_id)
    }

    async fn seed_primary_request(
        pool: &sqlx::PgPool,
        session_id: i64,
        origin: &str,
        sha: &str,
        blocks: &serde_json::Value,
    ) -> anyhow::Result<i64> {
        let now = chrono::Utc::now();
        let request_id: i64 = sqlx::query_scalar(
            "INSERT INTO requests.requests (
                session_id, project_id, method_id, path, is_streaming, message_count,
                is_tool_loop, status_id, params, last_user_message, human_prompt,
                turn_type_id, turn_origin_id, created_at
             )
             SELECT $1, s.project_id, rm.id, '/v1/messages', false, 1, false, rs.id,
                    '{}'::jsonb, '', '', rtt.id, rto.id, $3
               FROM sessions.sessions s, requests.methods rm, requests.statuses rs,
                    requests.turn_types rtt, requests.turn_origins rto
              WHERE s.id = $1 AND rm.code = 'POST' AND rs.code = 'complete'
                AND rtt.code = 'user_prompt' AND rto.code = $2
             RETURNING id",
        )
        .bind(session_id)
        .bind(origin)
        .bind(now)
        .fetch_one(pool)
        .await?;

        sqlx::query!(
            "WITH known_prompt AS (
                 SELECT sp.id FROM system_prompts.system_prompts sp
                  WHERE sp.prompt_sha256 = $2
                  ORDER BY sp.id
                  LIMIT 1
             ),
             new_prompt AS (
                 INSERT INTO system_prompts.system_prompts
                        (prompt_sha256, blocks, first_seen_at, last_seen_at)
                 SELECT $2, $3, $4, $4
                  WHERE NOT EXISTS (SELECT 1 FROM known_prompt)
                 RETURNING id
             )
             INSERT INTO requests.system_prompts (request_id, prompt_sha256, system_prompt_id)
             SELECT $1, $2, prompt.id
               FROM (SELECT id FROM new_prompt UNION ALL SELECT id FROM known_prompt) prompt",
            request_id,
            sha,
            blocks,
            now,
        )
        .execute(pool)
        .await?;

        Ok(request_id)
    }

    fn sha_for(role_body: &str) -> String {
        prompt_sha256_from_block_text(role_body)
    }

    fn blocks_for(role_body: &str) -> serde_json::Value {
        json!([
            {"type": "text", "text": "x-anthropic-billing-header: cch=abc;"},
            {"type": "text", "text": "You are Claude Code."},
            {"type": "text", "text": role_body},
        ])
    }

    async fn seed_main_row(
        pool: &sqlx::PgPool,
        session_id: i64,
        request_id: i64,
        blocks: &serde_json::Value,
    ) -> anyhow::Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO system_prompts.session_prompts
                (session_id, request_id, lane, blocks, content_sha256)
             VALUES ($1, $2, 'main', $3, $4) RETURNING id",
        )
        .bind(session_id)
        .bind(request_id)
        .bind(blocks)
        .bind(session_prompt_content_sha256(blocks))
        .fetch_one(pool)
        .await?;
        Ok(id)
    }

    #[tokio::test]
    async fn repair_repoints_a_row_claimed_by_a_single_observation() -> anyhow::Result<()> {
        let Some(db) = test_support::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = seed_session(&handle.pool).await?;

        let sidecar_blocks = blocks_for("name this session");
        let real_blocks = blocks_for("the session's actual role body");

        let sidecar_request = seed_primary_request(
            &handle.pool,
            session_id,
            "primary_first",
            &sha_for("name this session"),
            &sidecar_blocks,
        )
        .await?;
        let first_real = seed_primary_request(
            &handle.pool,
            session_id,
            "primary_first",
            &sha_for("the session's actual role body"),
            &real_blocks,
        )
        .await?;
        for _ in 0..3 {
            seed_primary_request(
                &handle.pool,
                session_id,
                "primary",
                &sha_for("the session's actual role body"),
                &real_blocks,
            )
            .await?;
        }

        let ledger_id =
            seed_main_row(&handle.pool, session_id, sidecar_request, &sidecar_blocks).await?;

        let found = handle.find_misattributed_main_lane_rows().await?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].winner_request_id, first_real);
        assert_eq!(found[0].winner_occurrences, 4);

        let outcome = handle.repair_main_lane_session_prompts(false).await?;
        assert_eq!(outcome.repaired, 1);
        assert_eq!(outcome.uncorroborated, 0);

        let (request_id, blocks): (i64, serde_json::Value) = sqlx::query_as(
            "SELECT request_id, blocks FROM system_prompts.session_prompts WHERE id = $1",
        )
        .bind(ledger_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(request_id, first_real);
        assert_eq!(blocks, real_blocks);

        assert!(handle.find_misattributed_main_lane_rows().await?.is_empty());

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn repair_leaves_uncorroborated_rows_alone() -> anyhow::Result<()> {
        let Some(db) = test_support::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = seed_session(&handle.pool).await?;

        let earlier_blocks = blocks_for("prompt a");
        seed_primary_request(
            &handle.pool,
            session_id,
            "primary_first",
            &sha_for("prompt a"),
            &earlier_blocks,
        )
        .await?;
        let later_blocks = blocks_for("prompt b");
        let later = seed_primary_request(
            &handle.pool,
            session_id,
            "primary",
            &sha_for("prompt b"),
            &later_blocks,
        )
        .await?;

        let ledger_id = seed_main_row(&handle.pool, session_id, later, &later_blocks).await?;

        let found = handle.find_misattributed_main_lane_rows().await?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].winner_occurrences, 1);

        let outcome = handle.repair_main_lane_session_prompts(false).await?;
        assert_eq!(outcome.repaired, 0);
        assert_eq!(outcome.uncorroborated, 1);

        let request_id: i64 = sqlx::query_scalar(
            "SELECT request_id FROM system_prompts.session_prompts WHERE id = $1",
        )
        .bind(ledger_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(request_id, later);

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn repair_dry_run_writes_nothing() -> anyhow::Result<()> {
        let Some(db) = test_support::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = seed_session(&handle.pool).await?;

        let sidecar_blocks = blocks_for("name this session");
        let real_blocks = blocks_for("the real body");
        let sidecar = seed_primary_request(
            &handle.pool,
            session_id,
            "primary_first",
            &sha_for("name this session"),
            &sidecar_blocks,
        )
        .await?;
        for _ in 0..2 {
            seed_primary_request(
                &handle.pool,
                session_id,
                "primary",
                &sha_for("the real body"),
                &real_blocks,
            )
            .await?;
        }
        let ledger_id = seed_main_row(&handle.pool, session_id, sidecar, &sidecar_blocks).await?;

        let outcome = handle.repair_main_lane_session_prompts(true).await?;
        assert_eq!(outcome.repaired, 1);

        let request_id: i64 = sqlx::query_scalar(
            "SELECT request_id FROM system_prompts.session_prompts WHERE id = $1",
        )
        .bind(ledger_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(request_id, sidecar);

        db.cleanup().await?;
        Ok(())
    }
}
