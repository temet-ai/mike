use serde_json::Value as JsonValue;

use crate::extract::request::{TurnOrigin, extract_turn_origin, extract_turn_type};

use super::PersistHandle;
use super::error::PersistError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnOriginRepairOutcome {
    pub restamped: u64,
    pub unchanged: u64,
    pub batches: u64,
}

impl PersistHandle {
    pub async fn restamp_misclassified_sidecar_origins(
        &self,
        batch_size: i64,
        dry_run: bool,
    ) -> Result<TurnOriginRepairOutcome, PersistError> {
        let mut total = TurnOriginRepairOutcome::default();
        let mut cursor: i64 = 0;
        loop {
            let (batch, next) = self
                .restamp_sidecar_origins_batch(batch_size, cursor, dry_run)
                .await?;
            let Some(next) = next else {
                return Ok(total);
            };
            total.restamped += batch.restamped;
            total.unchanged += batch.unchanged;
            total.batches += 1;
            cursor = next;
        }
    }

    pub async fn demote_duplicate_first_turns(
        &self,
        dry_run: bool,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("demote_duplicate_first_turns", async move {
            if dry_run {
                let count = sqlx::query_scalar!(
                    r#"
                    SELECT count(*) AS "count!"
                      FROM (
                        SELECT row_number() OVER (
                                 PARTITION BY r.session_id ORDER BY r.id
                               ) AS rn
                          FROM requests.requests r
                          JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                         WHERE o.code = 'primary_first'
                      ) ranked
                     WHERE ranked.rn > 1
                    "#
                )
                .fetch_one(&self.pool)
                .await?;
                return Ok(count as u64);
            }

            let result = sqlx::query!(
                r#"
                WITH ranked AS (
                    SELECT r.id,
                           row_number() OVER (
                               PARTITION BY r.session_id ORDER BY r.id
                           ) AS rn
                      FROM requests.requests r
                      JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                     WHERE o.code = 'primary_first'
                )
                UPDATE requests.requests r
                   SET turn_origin_id = p.id
                  FROM ranked, requests.turn_origins p
                 WHERE ranked.rn > 1
                   AND r.id = ranked.id
                   AND p.code = 'primary'
                "#
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    async fn restamp_sidecar_origins_batch(
        &self,
        batch_size: i64,
        after_request_id: i64,
        dry_run: bool,
    ) -> Result<(TurnOriginRepairOutcome, Option<i64>), PersistError> {
        crate::metrics::time_db("restamp_sidecar_origins_batch", async move {
            let mut outcome = TurnOriginRepairOutcome::default();

            let candidates = sqlx::query!(
                r#"
                SELECT r.id AS "id!", r.request_body AS "request_body!"
                  FROM requests.requests r
                  JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                 WHERE r.id > $2
                   AND o.code IN ('primary', 'primary_first')
                   AND r.message_count <= 2
                   AND r.request_body IS NOT NULL
                 ORDER BY r.id ASC
                 LIMIT $1
                "#,
                batch_size,
                after_request_id,
            )
            .fetch_all(&self.pool)
            .await?;

            let Some(highest) = candidates.last().map(|r| r.id) else {
                return Ok((outcome, None));
            };

            let mut restamp: Vec<(i64, String)> = Vec::new();
            for row in candidates {
                let Ok(body) = serde_json::from_slice::<JsonValue>(&row.request_body) else {
                    outcome.unchanged += 1;
                    continue;
                };
                let turn_type = extract_turn_type(&body);
                let system_prompt = crate::extract::request::extract_system_prompt(&body);
                let model = crate::extract::request::extract_model(&body);
                let origin = extract_turn_origin(
                    &body,
                    system_prompt.as_deref(),
                    model.as_deref(),
                    &turn_type,
                    true,
                    None,
                );
                let is_sidecar = matches!(
                    origin,
                    TurnOrigin::TitleSidecar
                        | TurnOrigin::AdvisorSidecar
                        | TurnOrigin::QuotaProbe
                        | TurnOrigin::MonitorSidecar
                        | TurnOrigin::CountTokensSidecar
                        | TurnOrigin::WebFetchSidecar
                        | TurnOrigin::NextInputSuggestionSidecar
                );
                if is_sidecar {
                    restamp.push((row.id, origin.to_string()));
                } else {
                    outcome.unchanged += 1;
                }
            }

            outcome.restamped = restamp.len() as u64;

            if !dry_run && !restamp.is_empty() {
                let ids: Vec<i64> = restamp.iter().map(|(id, _)| *id).collect();
                let codes: Vec<String> = restamp.into_iter().map(|(_, code)| code).collect();
                sqlx::query!(
                    "UPDATE requests.requests r
                        SET turn_origin_id = o.id
                       FROM UNNEST($1::bigint[], $2::text[]) AS u(request_id, code)
                       JOIN requests.turn_origins o ON o.code = u.code
                      WHERE r.id = u.request_id",
                    &ids,
                    &codes,
                )
                .execute(&self.pool)
                .await?;
            }

            Ok((outcome, Some(highest)))
        })
        .await
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::store::persistence::test_support;

    async fn seed_session_with_first_turns(pool: &sqlx::PgPool, n: usize) -> anyhow::Result<i64> {
        let now = chrono::Utc::now();
        let project_id: i64 = sqlx::query_scalar(
            "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
             VALUES ($1, $1, $2, $2) RETURNING id",
        )
        .bind(format!("/tmp/first-turns-{}", uuid::Uuid::new_v4()))
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

        for _ in 0..n {
            sqlx::query!(
                "INSERT INTO requests.requests (
                    session_id, project_id, method_id, path, is_streaming, message_count,
                    is_tool_loop, status_id, params, last_user_message, human_prompt,
                    turn_type_id, turn_origin_id, created_at
                 )
                 SELECT $1, $2, rm.id, '/v1/messages', false, 1, false, rs.id,
                        '{}'::jsonb, '', '', rtt.id, rto.id, $3
                   FROM requests.methods rm, requests.statuses rs,
                        requests.turn_types rtt, requests.turn_origins rto
                  WHERE rm.code = 'POST' AND rs.code = 'complete'
                    AND rtt.code = 'user_prompt' AND rto.code = 'primary_first'",
                session_id,
                project_id,
                now,
            )
            .execute(pool)
            .await?;
        }
        Ok(session_id)
    }

    async fn first_turn_count(pool: &sqlx::PgPool, session_id: i64) -> anyhow::Result<i64> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM requests.requests r
               JOIN requests.turn_origins o ON o.id = r.turn_origin_id
              WHERE r.session_id = $1 AND o.code = 'primary_first'",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await?;
        Ok(n)
    }

    #[tokio::test]
    async fn demotion_leaves_one_first_turn_and_is_idempotent() -> anyhow::Result<()> {
        let Some(db) = test_support::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = seed_session_with_first_turns(&handle.pool, 5).await?;
        assert_eq!(first_turn_count(&handle.pool, session_id).await?, 5);

        assert_eq!(handle.demote_duplicate_first_turns(true).await?, 4);
        assert_eq!(
            first_turn_count(&handle.pool, session_id).await?,
            5,
            "a dry run writes nothing"
        );

        assert_eq!(handle.demote_duplicate_first_turns(false).await?, 4);
        assert_eq!(first_turn_count(&handle.pool, session_id).await?, 1);

        let kept: i64 = sqlx::query_scalar(
            "SELECT r.id FROM requests.requests r
               JOIN requests.turn_origins o ON o.id = r.turn_origin_id
              WHERE r.session_id = $1 AND o.code = 'primary_first'",
        )
        .bind(session_id)
        .fetch_one(&handle.pool)
        .await?;
        let earliest: i64 =
            sqlx::query_scalar("SELECT min(id) FROM requests.requests WHERE session_id = $1")
                .bind(session_id)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(kept, earliest);

        assert_eq!(handle.demote_duplicate_first_turns(false).await?, 0);

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn demotion_leaves_a_well_formed_session_alone() -> anyhow::Result<()> {
        let Some(db) = test_support::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = seed_session_with_first_turns(&handle.pool, 1).await?;

        assert_eq!(handle.demote_duplicate_first_turns(false).await?, 0);
        assert_eq!(first_turn_count(&handle.pool, session_id).await?, 1);

        db.cleanup().await?;
        Ok(())
    }
}
