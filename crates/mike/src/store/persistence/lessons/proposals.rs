pub(crate) mod queue;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgConnection;

use super::placement::PlacementWrite;
use super::shape::insert_shape_finding;
use super::write_time::{PlannedRelation, insert_planned_relations};
use crate::lesson_shape::Assessment;
use crate::store::persistence::PersistError;
use crate::store::persistence::change_log::{NewChange, append_change_in};
use crate::store::persistence::operator_turns::vector_to_text_literal;

#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("persistence error: {0}")]
    Persist(#[from] PersistError),
    #[error("no such lesson candidate")]
    NotFound,
    #[error("lesson is not a pending candidate")]
    NotPending,
}

impl From<sqlx::Error> for ProposalError {
    fn from(e: sqlx::Error) -> Self {
        ProposalError::Persist(PersistError::from(e))
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CorrectionCandidate {
    pub verdict_id: i64,
    pub operator_turn_id: i64,
    pub session_id: i64,
    pub project_id: i64,
    pub confidence: f32,
    pub rationale: String,
    pub unmet_ask: String,
    pub user_prompt: Option<String>,
    pub final_assistant_text: Option<String>,
    pub correcting_message: Option<String>,
}

pub struct EvidenceWrite<'a> {
    pub verdict_id: i64,
    pub operator_turn_id: i64,
    pub lesson_id: Option<i64>,
    pub similar: &'a Value,
    pub model: &'a str,
    pub cost_usd: Option<f64>,
    pub generation_id: Option<&'a str>,
    pub reason: &'a str,
}

pub(super) async fn insert_evidence(
    conn: &mut PgConnection,
    ev: &EvidenceWrite<'_>,
) -> Result<i64, PersistError> {
    let id = sqlx::query_scalar!(
        "INSERT INTO memory.lesson_proposal_evidence
            (lesson_id, verdict_id, operator_turn_id, similar_corrections, model, cost_usd, generation_id, reason)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (verdict_id) DO UPDATE
            SET lesson_id = EXCLUDED.lesson_id, similar_corrections = EXCLUDED.similar_corrections,
                model = EXCLUDED.model, cost_usd = EXCLUDED.cost_usd,
                generation_id = EXCLUDED.generation_id, reason = EXCLUDED.reason,
                created_at = EXCLUDED.created_at
            WHERE EXISTS (SELECT 1 FROM memory.lesson_proposal_recovery recovery
                          WHERE recovery.verdict_id = EXCLUDED.verdict_id
                            AND recovery.recovery_state = 'needs_reproposal')
            RETURNING id",
        ev.lesson_id,
        ev.verdict_id,
        ev.operator_turn_id,
        ev.similar,
        ev.model,
        ev.cost_usd,
        ev.generation_id,
        ev.reason,
    )
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE memory.lesson_proposal_jobs
         SET status = CASE WHEN $2::bigint IS NULL THEN 'rejected' ELSE 'applied' END,
             updated_at = NOW() WHERE verdict_id = $1",
        ev.verdict_id,
        ev.lesson_id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!("UPDATE memory.lesson_proposal_recovery SET recovery_state = 'reproposed' WHERE verdict_id = $1", ev.verdict_id).execute(conn).await?;
    Ok(id)
}

pub struct NewCandidateLesson<'a> {
    pub project_id: i64,
    pub kind: &'a str,
    pub text: &'a str,
    pub source_turn_id: i64,
    pub session_db_id: i64,
    pub request_id: Option<i64>,
    pub embedding: &'a [f32],
    pub embedding_model: &'a str,
    pub shape: Option<&'a Assessment>,
    pub relations: &'a [PlannedRelation],
    pub placement: Option<PlacementWrite<'a>>,
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn select_correction_candidates(
        self,
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<CorrectionCandidate>, PersistError> {
        crate::metrics::time_db("select_correction_candidates", async move {
            let rows = sqlx::query_as!(
                CorrectionCandidate,
                r#"SELECT v.verdict_id AS "verdict_id!", v.operator_turn_id AS "operator_turn_id!",
                          v.session_id AS "session_id!", v.project_id AS "project_id!",
                          v.confidence AS "confidence!", v.rationale AS "rationale!",
                          v.unmet_ask AS "unmet_ask!", v.user_prompt, v.final_assistant_text,
                          v.correcting_message
                     FROM memory.lesson_eligible_corrections_v v
                    WHERE v.created_at >= $1
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.lesson_proposal_evidence e
                          LEFT JOIN memory.lesson_proposal_recovery recovery
                            ON recovery.verdict_id = e.verdict_id AND recovery.recovery_state = 'needs_reproposal'
                          WHERE e.verdict_id = v.verdict_id AND recovery.verdict_id IS NULL
                      )
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.lesson_proposal_jobs j WHERE j.verdict_id = v.verdict_id
                      )
                    ORDER BY v.created_at ASC, v.verdict_id ASC
                    LIMIT $2"#,
                since,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn record_evidence(self, ev: EvidenceWrite<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("record_lesson_proposal_evidence", async move {
            let mut tx = self.pool.begin().await?;
            let id = insert_evidence(&mut tx, &ev).await?;
            tx.commit().await?;
            Ok(id)
        })
        .await
    }

    pub async fn create_candidate_lesson(
        self,
        new: NewCandidateLesson<'_>,
        evidence: EvidenceWrite<'_>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("create_candidate_lesson", async move {
            let normalised = new.text.trim().to_lowercase();
            let text_sha256: Vec<u8> = sha256_bytes(normalised.as_bytes());
            let vec_literal = vector_to_text_literal(new.embedding);
            let mut tx = self.pool.begin().await?;
            let lesson_id = sqlx::query_scalar!(
                "INSERT INTO memory.lessons
                    (project_id, kind_id, text, text_sha256, stage_id, source_turn_id, source_id,
                     session_id, request_id, embedding, embedding_model, embedded_at,
                     placement_id, placement_covered_by, placement_reason, placed_at)
                 SELECT $1::bigint, lk.id, $3::text, $4::bytea, ls.id, $5::bigint, lsrc.id,
                        $6::bigint, $7::bigint, $8::text::vector, $9::text, NOW(),
                        lp.id, CASE WHEN lp.id IS NULL THEN NULL ELSE $11::text END,
                        CASE WHEN lp.id IS NULL THEN NULL ELSE $12::text END,
                        CASE WHEN lp.id IS NULL THEN NULL ELSE NOW() END
                   FROM memory.lesson_kinds lk
                  CROSS JOIN memory.lesson_stages ls
                  CROSS JOIN memory.lesson_sources lsrc
                   LEFT JOIN memory.lesson_placements lp ON lp.code = $10::text
                  WHERE lk.code = $2 AND ls.code = 'raw' AND lsrc.code = 'judge'
                 RETURNING id",
                new.project_id,
                new.kind,
                new.text,
                text_sha256,
                new.source_turn_id,
                new.session_db_id,
                new.request_id,
                vec_literal,
                new.embedding_model,
                new.placement.map(|p| p.home),
                new.placement.and_then(|p| p.covered_by),
                new.placement.map(|p| p.reason),
            )
            .fetch_one(&mut *tx)
            .await?;
            let mut ev = evidence;
            ev.lesson_id = Some(lesson_id);
            insert_evidence(&mut tx, &ev).await?;
            if let Some(assessment) = new.shape {
                insert_shape_finding(&mut tx, lesson_id, assessment, true).await?;
            }
            insert_planned_relations(&mut tx, lesson_id, ev.model, new.relations).await?;
            tx.commit().await?;
            Ok(lesson_id)
        })
        .await
    }

    pub async fn reinforce_lesson(
        self,
        lesson_id: i64,
        evidence: EvidenceWrite<'_>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("reinforce_lesson", async move {
            let mut tx = self.pool.begin().await?;
            sqlx::query!(
                "UPDATE memory.lessons
                    SET consolidation_count = consolidation_count + 1,
                        last_consolidated_at = NOW()
                  WHERE id = $1",
                lesson_id
            )
            .execute(&mut *tx)
            .await?;
            let mut ev = evidence;
            ev.lesson_id = Some(lesson_id);
            insert_evidence(&mut tx, &ev).await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }

    pub async fn count_lesson_evidence(self, lesson_id: i64) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_lesson_evidence", async move {
            let count = sqlx::query_scalar!(
                r#"SELECT count(*) AS "count!" FROM memory.lesson_proposal_evidence WHERE lesson_id = $1"#,
                lesson_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(count)
        })
        .await
    }

    pub async fn approve_lesson_candidate(
        self,
        lesson_id: i64,
        text_override: Option<&str>,
        decided_by: &str,
    ) -> Result<(), ProposalError> {
        crate::metrics::time_db("approve_lesson_candidate", async move {
            let mut tx = self.pool.begin().await.map_err(PersistError::from)?;
            let row = sqlx::query!(
                "SELECT ls.code AS stage, l.text
                   FROM memory.lessons l JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                  WHERE l.id = $1 FOR UPDATE OF l",
                lesson_id,
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ProposalError::NotFound)?;
            let (stage, old_text) = (row.stage, row.text);
            if stage != "raw" {
                return Err(ProposalError::NotPending);
            }
            let new_text = text_override.unwrap_or(old_text.as_str());
            sqlx::query!(
                "UPDATE memory.lessons l SET stage_id = daily_s.id, text = $2
                   FROM memory.lesson_stages daily_s
                  WHERE daily_s.code = 'daily' AND l.id = $1",
                lesson_id,
                new_text
            )
            .execute(&mut *tx)
            .await?;
            let subject_key = lesson_id.to_string();
            let source_json = change_source_json(decided_by);
            append_change_in(
                &mut tx,
                &NewChange {
                    subject_kind: "lesson_decision",
                    subject_key: &subject_key,
                    op: "update",
                    old_value: Some(&json!({"stage": "raw", "text": old_text})),
                    new_value: Some(
                        &json!({"stage": "daily", "text": new_text, "decided_by": decided_by}),
                    ),
                    source_json: &source_json,
                },
            )
            .await?;
            tx.commit().await.map_err(PersistError::from)?;
            Ok(())
        })
        .await
    }

    pub async fn reject_lesson_candidate(
        self,
        lesson_id: i64,
        api_reason: &str,
        note: Option<&str>,
    ) -> Result<(), ProposalError> {
        crate::metrics::time_db("reject_lesson_candidate", async move {
            let mut tx = self.pool.begin().await.map_err(PersistError::from)?;
            let stage = sqlx::query_scalar!(
                "SELECT ls.code
                   FROM memory.lessons l JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                  WHERE l.id = $1 FOR UPDATE OF l",
                lesson_id,
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ProposalError::NotFound)?;
            if stage != "raw" {
                return Err(ProposalError::NotPending);
            }
            sqlx::query!(
                "UPDATE memory.lessons l
                    SET stage_id = archived_s.id, archived_at = NOW(), archived_reason_id = ar.id
                   FROM memory.lesson_stages archived_s, memory.lesson_archived_reasons ar
                  WHERE archived_s.code = 'archived' AND ar.code = 'rejected_by_operator'
                    AND l.id = $1",
                lesson_id
            )
            .execute(&mut *tx)
            .await?;
            let subject_key = lesson_id.to_string();
            let source_json = change_source_json("operator");
            append_change_in(
                &mut tx,
                &NewChange {
                    subject_kind: "lesson_decision",
                    subject_key: &subject_key,
                    op: "update",
                    old_value: Some(&json!({"stage": "raw"})),
                    new_value: Some(&json!({"stage": "archived",
                        "archived_reason": "rejected_by_operator",
                        "api_reason": api_reason, "note": note})),
                    source_json: &source_json,
                },
            )
            .await?;
            tx.commit().await.map_err(PersistError::from)?;
            Ok(())
        })
        .await
    }
}

fn change_source_json(decided_by: &str) -> String {
    json!({"via": "mike", "detail": {"decided_by": decided_by}}).to_string()
}

fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes).to_vec()
}
