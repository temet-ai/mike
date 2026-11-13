use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CorrectionCandidate;
use crate::lesson_proposer::prompt::ProposedLesson;
use crate::store::persistence::PersistError;

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedProposal {
    pub candidate: CorrectionCandidate,
    pub proposed: ProposedLesson,
    pub similar: Value,
    pub model: String,
    pub cost_usd: Option<f64>,
    pub generation_id: Option<String>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn lock_lesson_proposer(
        self,
    ) -> Result<Option<sqlx::Transaction<'static, sqlx::Postgres>>, PersistError> {
        let mut tx = self.pool.begin().await?;
        let locked: bool =
            sqlx::query_scalar!(r#"SELECT pg_try_advisory_xact_lock(739104721) AS "locked!""#)
                .fetch_one(&mut *tx)
                .await?;
        if locked { Ok(Some(tx)) } else { Ok(None) }
    }

    pub async fn reserve_lesson_proposer_call(self, cap: i64) -> Result<bool, PersistError> {
        if cap <= 0 {
            return Ok(false);
        }
        let consumed: Option<i64> = sqlx::query_scalar!(
            "INSERT INTO memory.lesson_proposer_daily_budget (day, consumed)
             VALUES ((NOW() AT TIME ZONE 'UTC')::date, 1)
             ON CONFLICT (day) DO UPDATE
                SET consumed = memory.lesson_proposer_daily_budget.consumed + 1
              WHERE memory.lesson_proposer_daily_budget.consumed < $1
             RETURNING consumed",
            cap
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(consumed.is_some())
    }

    pub async fn save_lesson_proposal(self, saved: &SavedProposal) -> Result<(), PersistError> {
        sqlx::query!(
            "INSERT INTO memory.lesson_proposal_jobs
                (verdict_id, candidate, proposed, \"similar\", model, cost_usd, generation_id, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'awaiting_embedding')
             ON CONFLICT (verdict_id) DO NOTHING",
            saved.candidate.verdict_id,
            sqlx::types::Json(&saved.candidate) as _,
            sqlx::types::Json(&saved.proposed) as _,
            &saved.similar,
            &saved.model,
            saved.cost_usd,
            saved.generation_id.as_deref()
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn reject_obsolete_lesson_proposals(self) -> Result<(), PersistError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query!(
            "UPDATE memory.lesson_proposal_jobs j SET status = 'rejected', updated_at = NOW()
             WHERE j.status = 'awaiting_embedding' AND NOT EXISTS (
                 SELECT 1 FROM memory.lesson_eligible_corrections_v v WHERE v.verdict_id = j.verdict_id
             )",
        ).execute(&mut *tx).await?;
        sqlx::query!(
            "UPDATE memory.lesson_proposal_recovery r SET recovery_state = 'obsolete',
                 disposition_reason = 'No longer the current eligible human correction'
             WHERE r.recovery_state = 'needs_reproposal' AND NOT EXISTS (
                 SELECT 1 FROM memory.lesson_eligible_corrections_v v WHERE v.verdict_id = r.verdict_id
             )",
        ).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn pending_lesson_proposals(
        self,
        limit: i64,
    ) -> Result<Vec<SavedProposal>, PersistError> {
        self.reject_obsolete_lesson_proposals().await?;
        let rows = sqlx::query_scalar!(
            r#"SELECT jsonb_build_object('candidate', candidate, 'proposed', proposed,
                'similar', "similar", 'model', model, 'cost_usd', cost_usd,
                'generation_id', generation_id) AS "saved!: sqlx::types::Json<SavedProposal>"
             FROM memory.lesson_proposal_jobs
             WHERE status = 'awaiting_embedding' AND next_attempt_at <= NOW()
             ORDER BY updated_at, verdict_id LIMIT $1"#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.0).collect())
    }

    pub async fn touch_lesson_proposal(self, verdict_id: i64) -> Result<(), PersistError> {
        sqlx::query!(
            "UPDATE memory.lesson_proposal_jobs SET updated_at = NOW(),
                retry_count = least(retry_count + 1, 16),
                next_attempt_at = NOW() + make_interval(secs => least(3600, 15 * power(2, least(retry_count + 1, 12))::integer))
             WHERE verdict_id = $1",
            verdict_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
