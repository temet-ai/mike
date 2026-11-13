use super::TurnJudgmentStamp;
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn reap_idle_open_turns(
        &self,
        idle_threshold: std::time::Duration,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("reap_idle_open_turns", async move {
            let idle_threshold_pg = sqlx::postgres::types::PgInterval {
                months: 0,
                days: 0,
                microseconds: idle_threshold.as_micros() as i64,
            };
            let mut tx = self.pool.begin().await?;
            let turn_ids: Vec<i64> = sqlx::query_scalar!(
                r#"WITH stale AS (
                     SELECT t.id
                       FROM memory.operator_turns t
                       JOIN sessions.sessions s ON s.id = t.session_id
                      WHERE t.closed_at IS NULL
                        AND s.last_seen < $2::timestamptz - $1::interval
                      FOR UPDATE OF t SKIP LOCKED
                 )
                 UPDATE memory.operator_turns t
                    SET closed_at     = NOW(),
                        classified_at = NOW()
                   FROM stale
                  WHERE t.id = stale.id
                  RETURNING t.id AS "id!""#,
                idle_threshold_pg,
                now,
            )
            .fetch_all(&mut *tx)
            .await?;

            for turn_id in &turn_ids {
                crate::store::persistence::turn_lifecycle::materialize_turn_aggregates(
                    &mut tx, *turn_id,
                )
                .await?;
            }

            tx.commit().await?;
            Ok(turn_ids)
        })
        .await
    }

    pub async fn stamp_turn_judgment(
        &self,
        stamp: TurnJudgmentStamp<'_>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("stamp_turn_judgment", async move {
            let TurnJudgmentStamp {
                turn_id,
                outcome_code,
                judge_model,
                rubric_version,
                detail,
                bundle_id,
            } = stamp;
            let (rationale, unmet_ask, evidence_quality) = match detail {
                Some(d) => (d.rationale, d.unmet_ask, d.evidence_quality),
                None => (None, None, None),
            };
            let result = sqlx::query!(
                "UPDATE memory.operator_turns t
                    SET outcome_id           = tot.id,
                        classified_at        = NOW(),
                        judge_model          = $2,
                        judge_rubric_version = $3,
                        judge_failure_reason  = NULL,
                        judge_attempt_count   = 0,
                        judge_next_attempt_at = NULL,
                        rationale             = $5,
                        unmet_ask             = $6,
                        evidence_quality      = $7,
                        outcome_bundle_id     = $8
                   FROM memory.turn_outcomes tot
                  WHERE tot.code = $4
                    AND t.id     = $1
                    AND t.outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = 'unknown')",
                turn_id,
                judge_model,
                rubric_version,
                outcome_code,
                rationale,
                unmet_ask,
                evidence_quality,
                bundle_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub async fn mark_judge_failed(
        &self,
        turn_id: i64,
        reason: &str,
        backoff_base_secs: i64,
        backoff_max_secs: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("mark_judge_failed", async move {
            let result = sqlx::query!(
                "UPDATE memory.operator_turns t
                    SET judge_failure_reason  = $2,
                        judge_attempt_count   = t.judge_attempt_count + 1,
                        judge_next_attempt_at = CASE
                            WHEN $2 = 'transient_exhausted'
                            THEN NOW() + (INTERVAL '1 second' * LEAST(
                                $4::int8::float8,
                                $3::int8::float8 * POWER(2::float8, t.judge_attempt_count::float8)
                            ))
                            ELSE NULL
                        END,
                        judge_queued_at = CASE WHEN $2 = 'permanent' THEN NULL
                                               ELSE t.judge_queued_at END
                  WHERE t.id = $1
                    AND t.outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = 'unknown')",
                turn_id,
                reason,
                backoff_base_secs,
                backoff_max_secs,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }
}
