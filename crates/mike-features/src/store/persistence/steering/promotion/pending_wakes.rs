use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct DueWakeRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub session_id: i64,
    pub session_uuid: uuid::Uuid,
    pub question: Option<String>,
    pub anchor_turn_id: Option<i64>,
    pub evaluation_id: i64,
    pub evaluated_turn_id: i64,
    pub mode: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub delivered: bool,
    pub reverted: bool,
    pub revert_reason: Option<String>,
    pub payload_text: String,
    pub turn_closed: bool,
    pub moved_on: bool,
    pub stream_open: bool,
    pub pushed_last_hour: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ReachDueRow {
    pub id: i64,
    pub session_id: i64,
    pub question: Option<String>,
    pub turn_closed: bool,
    pub moved_on: bool,
    pub stream_open: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WakeFinal {
    pub id: i64,
    pub state: &'static str,
    pub expire: bool,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn claim_due_wakes(self, limit: i64) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("claim_due_wakes", async move {
            Ok(sqlx::query_scalar!(
                r#"WITH due AS (
                       SELECT id FROM steering.interventions
                        WHERE wake_state = 'pending' AND deliver_after <= now()
                        ORDER BY deliver_after
                        LIMIT $1
                        FOR UPDATE SKIP LOCKED
                   )
                   UPDATE steering.interventions i
                      SET wake_state = 'delivering', wake_claimed_at = now(),
                          wake_attempts = i.wake_attempts + 1
                     FROM due
                    WHERE i.id = due.id
                   RETURNING i.id AS "id!""#,
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn recover_stale_wakes(self) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("recover_stale_wakes", async move {
            Ok(sqlx::query_scalar!(
                r#"UPDATE steering.interventions
                      SET wake_state = 'abandoned', wake_resolved_at = now()
                    WHERE wake_state = 'delivering'
                      AND wake_claimed_at < now() - interval '120 seconds'
                   RETURNING id AS "id!""#,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn disable_all_pending_wakes(self) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("disable_all_pending_wakes", async move {
            Ok(sqlx::query_scalar!(
                r#"UPDATE steering.interventions
                      SET wake_state = 'disabled', wake_resolved_at = now(), expires_at = now()
                    WHERE wake_state = 'pending'
                   RETURNING id AS "id!""#,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn disable_pending_for_question(
        self,
        steerer_id: i64,
        question: &str,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("disable_pending_for_question", async move {
            Ok(sqlx::query_scalar!(
                r#"UPDATE steering.interventions i
                      SET wake_state = 'disabled', wake_resolved_at = now(), expires_at = now()
                     FROM steering.evaluations e
                    WHERE e.intervention_uuid = i.uuid AND e.steerer_id = $1 AND e.question = $2
                      AND i.wake_state = 'pending'
                   RETURNING i.id AS "id!""#,
                steerer_id,
                question,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn reject_pending_wake(
        self,
        evaluation: uuid::Uuid,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("reject_pending_wake", async move {
            Ok(sqlx::query_scalar!(
                r#"UPDATE steering.interventions i
                      SET wake_state = 'operator_rejected', wake_resolved_at = now(),
                          expires_at = now()
                     FROM steering.evaluations e
                    WHERE e.uuid = $1 AND e.intervention_uuid = i.uuid
                      AND i.wake_state = 'pending'
                   RETURNING i.id AS "id!""#,
                evaluation,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fetch_due_wake_context(
        self,
        ids: &[i64],
    ) -> Result<Vec<DueWakeRow>, PersistError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("fetch_due_wake_context", async move {
            Ok(sqlx::query_file_as!(
                DueWakeRow,
                "src/store/persistence/steering/promotion/due_wake_context.sql",
                ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn claim_reach_due(self, limit: i64) -> Result<Vec<ReachDueRow>, PersistError> {
        crate::metrics::time_db("claim_reach_due", async move {
            Ok(sqlx::query_file_as!(
                ReachDueRow,
                "src/store/persistence/steering/promotion/reach_due.sql",
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn finalise_wakes(self, finals: &[WakeFinal]) -> Result<u64, PersistError> {
        if finals.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = finals.iter().map(|f| f.id).collect();
        let states: Vec<String> = finals.iter().map(|f| f.state.to_owned()).collect();
        let expire: Vec<bool> = finals.iter().map(|f| f.expire).collect();
        crate::metrics::time_db("finalise_wakes", async move {
            let r = sqlx::query!(
                "UPDATE steering.interventions i
                    SET wake_state = u.state, wake_resolved_at = now(),
                        expires_at = CASE WHEN u.expire THEN now() ELSE i.expires_at END,
                        pushed_at = CASE WHEN u.state = 'pushed' THEN now() ELSE i.pushed_at END
                   FROM unnest($1::bigint[], $2::text[], $3::bool[]) AS u(id, state, expire)
                  WHERE i.id = u.id AND i.wake_state = 'delivering'",
                &ids,
                &states,
                &expire,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected())
        })
        .await
    }

    pub async fn return_wakes_to_pending(self, ids: &[i64]) -> Result<u64, PersistError> {
        if ids.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("return_wakes_to_pending", async move {
            Ok(sqlx::query!(
                "UPDATE steering.interventions
                    SET wake_state = 'pending'
                  WHERE id = ANY($1::bigint[]) AND wake_state = 'delivering'",
                ids,
            )
            .execute(&self.pool)
            .await?
            .rows_affected())
        })
        .await
    }

    pub async fn write_reach_at_due(
        self,
        rows: &[(i64, &'static str)],
    ) -> Result<u64, PersistError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
        let reach: Vec<String> = rows.iter().map(|r| r.1.to_owned()).collect();
        crate::metrics::time_db("write_reach_at_due", async move {
            Ok(sqlx::query!(
                "UPDATE steering.evaluations e
                    SET reach_at_due = u.reach
                   FROM unnest($1::bigint[], $2::text[]) AS u(id, reach)
                  WHERE e.id = u.id AND e.reach_at_due IS NULL",
                &ids,
                &reach,
            )
            .execute(&self.pool)
            .await?
            .rows_affected())
        })
        .await
    }
}
