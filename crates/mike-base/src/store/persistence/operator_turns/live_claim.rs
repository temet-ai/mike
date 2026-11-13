use chrono::{DateTime, Utc};
use sqlx::postgres::PgArguments;
use sqlx::{Arguments, Postgres, QueryBuilder};

use super::eligibility::{eligible_without_verdict, queued_without_verdict};
use super::judge_claim::ClaimScope;
use super::projection::{PROJECTION_BATCH, ProjectionMode, ProjectionParams};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveClaim {
    pub bundle_id: i64,
    pub since: DateTime<Utc>,
    pub limit: i64,
}

pub const BACKLOG_COUNT_CAP: i64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveBacklog {
    pub eligible_without_verdict: i64,
    pub oldest_closed_at: Option<DateTime<Utc>>,
    pub capped: bool,
}

impl LiveBacklog {
    fn from_capped(counted: i64, oldest_closed_at: Option<DateTime<Utc>>) -> Self {
        Self {
            eligible_without_verdict: counted.min(BACKLOG_COUNT_CAP),
            oldest_closed_at,
            capped: counted > BACKLOG_COUNT_CAP,
        }
    }
}

fn bundle_query(
    head: &'static str,
    bundle_id: i64,
) -> Result<QueryBuilder<Postgres>, PersistError> {
    let mut arguments = PgArguments::default();
    arguments.add(bundle_id).map_err(sqlx::Error::Encode)?;
    Ok(QueryBuilder::with_arguments(head, arguments))
}

fn capped_backlog_query(
    bundle_id: i64,
    closed_since: Option<DateTime<Utc>>,
    closed_before: DateTime<Utc>,
) -> Result<QueryBuilder<Postgres>, PersistError> {
    let mut query = bundle_query(
        "SELECT count(*)::bigint, min(o.closed_at) FROM (SELECT t.closed_at ",
        bundle_id,
    )?;
    query.push(queued_without_verdict());
    if let Some(since) = closed_since {
        query.push(" AND t.closed_at >= ").push_bind(since);
    }
    query
        .push(" AND t.closed_at < ")
        .push_bind(closed_before)
        .push(" ORDER BY t.closed_at, t.id LIMIT ")
        .push_bind(BACKLOG_COUNT_CAP + 1)
        .push(") o");
    Ok(query)
}

fn reconcile_queue_query(bundle_id: i64) -> Result<QueryBuilder<Postgres>, PersistError> {
    let mut query = bundle_query("WITH want AS (SELECT t.id ", bundle_id)?;
    query.push(eligible_without_verdict()).push(
        "), queued AS (
            UPDATE memory.operator_turns u
               SET judge_queued_at = now()
              FROM want
             WHERE u.id = want.id AND u.judge_queued_at IS NULL
            RETURNING u.id
        ), dequeued AS (
            UPDATE memory.operator_turns u
               SET judge_queued_at = NULL
             WHERE u.judge_queued_at IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM want WHERE want.id = u.id)
            RETURNING u.id
        )
        SELECT (SELECT count(*) FROM queued)::bigint, (SELECT count(*) FROM dequeued)::bigint",
    );
    Ok(query)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgeQueueReconcile {
    pub queued: i64,
    pub dequeued: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveFailure<'a> {
    pub turn_id: i64,
    pub reason: &'a str,
    pub bundle_id: i64,
    pub backoff_base_secs: i64,
    pub backoff_max_secs: i64,
}

impl PersistHandle {
    pub async fn claim_live_candidates(&self, claim: LiveClaim) -> Result<Vec<i64>, PersistError> {
        let claimed = self.claim_live_candidates_dated(claim).await?;
        Ok(claimed.into_iter().map(|(id, _)| id).collect())
    }

    pub async fn claim_live_candidates_dated(
        &self,
        claim: LiveClaim,
    ) -> Result<Vec<(i64, DateTime<Utc>)>, PersistError> {
        crate::metrics::time_db("claim_live_candidates", async move {
            self.claim_judge_turns_dated(
                claim.bundle_id,
                ClaimScope::ClosedSince(claim.since),
                claim.limit,
            )
            .await
        })
        .await
    }

    pub async fn claim_live_candidates_for_session(
        &self,
        session_db_id: i64,
        bundle_id: i64,
        limit: i64,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("claim_live_candidates_for_session", async move {
            self.claim_judge_turns(bundle_id, ClaimScope::Session(session_db_id), limit)
                .await
        })
        .await
    }

    pub async fn mark_live_judge_failed(&self, f: LiveFailure<'_>) -> Result<bool, PersistError> {
        crate::metrics::time_db("mark_live_judge_failed", async move {
            let result = sqlx::query!(
                "UPDATE memory.operator_turns t
                    SET judge_failure_reason  = $2,
                        judge_attempt_count   = t.judge_attempt_count + 1,
                        judge_next_attempt_at = CASE
                            WHEN $2 = 'transient_exhausted'
                            THEN now() + (INTERVAL '1 second' * LEAST(
                                $5::int8::float8,
                                $4::int8::float8 * POWER(2::float8, t.judge_attempt_count::float8)
                            ))
                            ELSE NULL
                        END,
                        judge_queued_at       = CASE WHEN $2 = 'permanent' THEN NULL ELSE t.judge_queued_at END
                  WHERE t.id = $1
                    AND NOT EXISTS (
                        SELECT 1 FROM memory.turn_verdicts v
                          JOIN memory.turn_facts f ON f.operator_turn_id = v.operator_turn_id
                         WHERE v.operator_turn_id = t.id AND v.mode = 'live'
                           AND v.bundle_id = $3 AND v.reaction IS NOT NULL
                           AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id)",
                f.turn_id,
                f.reason,
                f.bundle_id,
                f.backoff_base_secs,
                f.backoff_max_secs,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub async fn reconcile_judge_queue(
        &self,
        bundle_id: i64,
    ) -> Result<JudgeQueueReconcile, PersistError> {
        crate::metrics::time_db("reconcile_judge_queue", async move {
            let (queued, dequeued) = reconcile_queue_query(bundle_id)?
                .build_query_as::<(i64, i64)>()
                .fetch_one(&self.pool)
                .await?;
            Ok(JudgeQueueReconcile { queued, dequeued })
        })
        .await
    }

    pub async fn clear_transient_judge_backoff(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("clear_transient_judge_backoff", async move {
            let result = sqlx::query!(
                "UPDATE memory.operator_turns
                    SET judge_next_attempt_at = NULL
                  WHERE judge_failure_reason = 'transient_exhausted'
                    AND judge_next_attempt_at > now()"
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    pub async fn live_backlog(
        &self,
        bundle_id: i64,
        closed_before: DateTime<Utc>,
    ) -> Result<LiveBacklog, PersistError> {
        crate::metrics::time_db("live_backlog", async move {
            let (eligible_without_verdict, oldest_closed_at) =
                capped_backlog_query(bundle_id, None, closed_before)?
                    .build_query_as::<(i64, Option<DateTime<Utc>>)>()
                    .fetch_one(&self.pool)
                    .await?;
            Ok(LiveBacklog::from_capped(
                eligible_without_verdict,
                oldest_closed_at,
            ))
        })
        .await
    }

    async fn live_window(&self) -> Result<Option<(i64, i64)>, PersistError> {
        use crate::settings::meta_turn_judge::{
            DEFAULT_LIVE_WINDOW_DAYS, LIVE_BUNDLE_ID_KEY, LIVE_WINDOW_DAYS_KEY,
        };
        crate::metrics::time_db("live_window", async move {
            let window = sqlx::query!(
                r#"SELECT coalesce(max((value #>> '{}')::bigint) FILTER (WHERE key = $1), 0) AS "bundle_id!",
                          coalesce(max((value #>> '{}')::bigint) FILTER (WHERE key = $2), $3) AS "window_days!"
                     FROM config.settings
                    WHERE key IN ($1, $2)"#,
                LIVE_BUNDLE_ID_KEY,
                LIVE_WINDOW_DAYS_KEY,
                DEFAULT_LIVE_WINDOW_DAYS,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok((window.bundle_id > 0).then_some((window.bundle_id, window.window_days)))
        })
        .await
    }

    pub async fn live_window_backlog(
        &self,
        now: DateTime<Utc>,
        closed_before: DateTime<Utc>,
    ) -> Result<Option<LiveBacklog>, PersistError> {
        let Some((bundle_id, window_days)) = self.live_window().await? else {
            return Ok(None);
        };
        let since = now - chrono::Duration::days(window_days);
        crate::metrics::time_db("live_window_backlog", async move {
            let (eligible_without_verdict, oldest_closed_at) =
                capped_backlog_query(bundle_id, Some(since), closed_before)?
                    .build_query_as::<(i64, Option<DateTime<Utc>>)>()
                    .fetch_one(&self.pool)
                    .await?;
            Ok(Some(LiveBacklog::from_capped(
                eligible_without_verdict,
                oldest_closed_at,
            )))
        })
        .await
    }

    pub async fn reproject_final_turns(
        &self,
        params: &ProjectionParams,
        window_days: i32,
    ) -> Result<Vec<(i64, String)>, PersistError> {
        let ids: Vec<i64> = crate::metrics::time_db("reproject_final_turns", async move {
            Ok::<_, PersistError>(
                sqlx::query_file_scalar!(
                    "src/store/persistence/operator_turns/final_turns.sql",
                    params.live_bundle_id,
                    window_days,
                    i64::try_from(PROJECTION_BATCH).unwrap_or(500),
                )
                .fetch_all(&self.pool)
                .await?,
            )
        })
        .await?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let reset = ProjectionParams {
            mode: ProjectionMode::ResetWithoutVerdict,
            ..*params
        };
        self.project_turn_outcomes(&ids, &reset).await
    }
}

#[cfg(test)]
#[path = "live_claim_tests.rs"]
mod tests;
