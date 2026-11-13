use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::PgConnection;

use super::proposals::{EvidenceWrite, insert_evidence};
use crate::store::persistence::PersistError;
use crate::store::persistence::operator_turns::vector_to_text_literal;

pub const WRITE_TIME_DETECTOR_VERSION: &str = "lesson-write-time-v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighbourLesson {
    pub id: i64,
    pub text: String,
    pub distance: f32,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub source_turn_id: Option<i64>,
    pub session_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedRelation {
    pub to_lesson_id: i64,
    pub kind: String,
    pub distance: f64,
    pub rationale: String,
    pub evidence: serde_json::Value,
}

pub struct NewSkippedDuplicate<'a> {
    pub project_id: i64,
    pub kind: &'a str,
    pub text: &'a str,
    pub source_turn_id: i64,
    pub session_db_id: i64,
    pub embedding: Option<(&'a [f32], &'a str)>,
    pub target_id: i64,
    pub model: &'a str,
    pub relation: &'a PlannedRelation,
}

fn sha_of(text: &str) -> Vec<u8> {
    Sha256::digest(text.trim().to_lowercase().as_bytes()).to_vec()
}

pub async fn insert_planned_relations(
    conn: &mut PgConnection,
    from_lesson_id: i64,
    model: &str,
    relations: &[PlannedRelation],
) -> Result<(), PersistError> {
    if relations.is_empty() {
        return Ok(());
    }
    let plan = serde_json::to_value(relations)
        .map_err(|e| PersistError::DataInvariant(format!("planned relations: {e}")))?;
    sqlx::query!(
        "INSERT INTO memory.lesson_relations
            (from_lesson_id, to_lesson_id, kind_id, method_id, status_id, candidate_distance,
             evidence_json, proposer_model, proposer_rationale, detector_version)
         SELECT $1, p.to_lesson_id, rk.id, rm.id, rs.id, p.distance, p.evidence, $2, p.rationale, $4
           FROM jsonb_to_recordset($3::jsonb)
                AS p(to_lesson_id bigint, kind text, distance float8, rationale text, evidence jsonb)
           JOIN memory.lesson_relation_kinds rk ON rk.code = p.kind
          CROSS JOIN memory.lesson_relation_methods rm
          CROSS JOIN memory.lesson_relation_statuses rs
          WHERE rm.code = 'write_time' AND rs.code = 'proposed'
         ON CONFLICT (from_lesson_id, to_lesson_id, kind_id) DO NOTHING",
        from_lesson_id,
        model,
        plan,
        WRITE_TIME_DETECTOR_VERSION,
    )
    .execute(conn)
    .await?;
    Ok(())
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn find_lesson_neighbours(
        self,
        project_id: i64,
        embedding: &[f32],
        max_distance: f32,
        limit: i64,
    ) -> Result<Vec<NeighbourLesson>, PersistError> {
        crate::metrics::time_db("find_lesson_neighbours", async move {
            let literal = vector_to_text_literal(embedding);
            Ok(sqlx::query_as!(
                NeighbourLesson,
                r#"SELECT l.id, l.text, (l.embedding <=> $2::text::vector)::float4 AS "distance!",
                          src.code AS "source!", l.created_at, l.source_turn_id, l.session_id
                     FROM memory.lessons l
                     JOIN memory.lesson_sources src ON src.id = l.source_id
                    WHERE l.project_id = $1
                      AND l.archived_at IS NULL
                      AND l.embedding IS NOT NULL
                      AND (l.embedding <=> $2::text::vector) <= $3
                      AND NOT EXISTS (
                          SELECT 1
                            FROM memory.lesson_relations r
                            JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id AND rk.code = 'supersedes'
                            JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id AND rs.code = 'confirmed'
                           WHERE r.to_lesson_id = l.id)
                    ORDER BY l.embedding <=> $2::text::vector
                    LIMIT $4"#,
                project_id,
                literal,
                f64::from(max_distance),
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn record_skipped_duplicate(
        self,
        skipped: NewSkippedDuplicate<'_>,
        evidence: EvidenceWrite<'_>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("record_skipped_duplicate", async move {
            let literal = skipped.embedding.map(|(v, _)| vector_to_text_literal(v));
            let model = skipped.embedding.map(|(_, m)| m);
            let mut tx = self.pool.begin().await?;
            let new_id = sqlx::query_scalar!(
                r#"INSERT INTO memory.lessons
                        (project_id, kind_id, text, text_sha256, stage_id, source_turn_id, source_id,
                         session_id, embedding, embedding_model, embedded_at, archived_at,
                         archived_reason_id, duplicate_of_id)
                   SELECT $1, lk.id, $3, $4, ls.id, $5, lsrc.id, $6,
                          $7::text::vector, $8::text, CASE WHEN $7::text IS NULL THEN NULL ELSE now() END,
                          now(), ar.id, $9
                     FROM memory.lesson_kinds lk, memory.lesson_stages ls,
                          memory.lesson_sources lsrc, memory.lesson_archived_reasons ar
                    WHERE lk.code = $2 AND ls.code = 'archived' AND lsrc.code = 'judge'
                      AND ar.code = 'duplicate'
                RETURNING id AS "id!""#,
                skipped.project_id,
                skipped.kind,
                skipped.text,
                sha_of(skipped.text),
                skipped.source_turn_id,
                skipped.session_db_id,
                literal,
                model,
                skipped.target_id,
            )
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query!(
                "UPDATE memory.lessons
                    SET consolidation_count = consolidation_count + 1, last_consolidated_at = now()
                  WHERE id = $1",
                skipped.target_id,
            )
            .execute(&mut *tx)
            .await?;
            let mut ev = evidence;
            ev.lesson_id = Some(skipped.target_id);
            insert_evidence(&mut tx, &ev).await?;
            insert_planned_relations(
                &mut tx,
                new_id,
                skipped.model,
                std::slice::from_ref(skipped.relation),
            )
            .await?;
            tx.commit().await?;
            Ok(new_id)
        })
        .await
    }

    pub async fn count_promotion_evidence(self, lesson_id: i64) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_promotion_evidence", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT count(*) AS "n!"
                     FROM memory.lesson_proposal_evidence e
                     JOIN (SELECT $1::bigint AS lesson_id
                           UNION
                           SELECT r.to_lesson_id
                             FROM memory.lesson_relations r
                             JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
                             JOIN memory.lesson_relation_methods m ON m.id = r.method_id
                            WHERE r.from_lesson_id = $1
                              AND k.code IN ('supersedes', 'refines')
                              AND m.code = 'write_time') backed
                       ON backed.lesson_id = e.lesson_id"#,
                lesson_id,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn confirm_judge_only_write_time_relations(self) -> Result<u64, PersistError> {
        crate::metrics::time_db("confirm_judge_only_write_time_relations", async move {
            Ok(sqlx::query!(
                "UPDATE memory.lesson_relations r
                    SET status_id = cs.id, decided_at = now(), decided_by = 'judge_reinforcement'
                   FROM memory.lesson_relation_statuses ps, memory.lesson_relation_statuses cs,
                        memory.lesson_relation_kinds k, memory.lesson_relation_methods m,
                        memory.lessons f, memory.lessons t,
                        memory.lesson_sources fs, memory.lesson_sources ts, memory.lesson_stages fst
                  WHERE ps.code = 'proposed' AND cs.code = 'confirmed' AND r.status_id = ps.id
                    AND k.id = r.kind_id AND k.code IN ('supersedes', 'refines', 'recurs')
                    AND m.id = r.method_id AND m.code = 'write_time'
                    AND f.id = r.from_lesson_id AND t.id = r.to_lesson_id
                    AND fs.id = f.source_id AND fs.code = 'judge'
                    AND ts.id = t.source_id AND ts.code = 'judge'
                    AND fst.id = f.stage_id
                    AND (fst.code IN ('daily', 'long_term') OR k.code = 'recurs')"
            )
            .execute(&self.pool)
            .await?
            .rows_affected())
        })
        .await
    }

    pub async fn apply_confirmed_lesson_relations(self) -> Result<u64, PersistError> {
        crate::metrics::time_db("apply_confirmed_lesson_relations", async move {
            let archived = sqlx::query_scalar!(
                r#"WITH due AS (
                     SELECT DISTINCT ON (r.to_lesson_id)
                            r.id AS relation_id, r.from_lesson_id AS new_id, r.to_lesson_id AS old_id
                       FROM memory.lesson_relations r
                       JOIN memory.lesson_relation_statuses s ON s.id = r.status_id AND s.code = 'confirmed'
                       JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id
                       JOIN memory.lesson_relation_methods m ON m.id = r.method_id
                       JOIN memory.lessons nl ON nl.id = r.from_lesson_id AND nl.archived_at IS NULL
                       JOIN memory.lessons ol ON ol.id = r.to_lesson_id AND ol.archived_at IS NULL
                       JOIN memory.lesson_stages ns ON ns.id = nl.stage_id
                       JOIN memory.lesson_sources nsrc ON nsrc.id = nl.source_id
                       LEFT JOIN memory.lesson_shape_findings open_shape
                         ON open_shape.lesson_id = nl.id AND open_shape.resolution IS NULL
                      WHERE (k.code = 'supersedes' OR (k.code = 'refines' AND m.code = 'write_time'))
                        AND (ns.code IN ('daily', 'long_term')
                             OR (ns.code = 'raw' AND nsrc.code = 'operator' AND open_shape.lesson_id IS NULL))
                      ORDER BY r.to_lesson_id, r.decided_at DESC NULLS LAST, r.id DESC
                 ),
                 archived AS (
                     UPDATE memory.lessons l
                        SET stage_id = a.id, archived_at = now(), archived_reason_id = ar.id
                       FROM due d, memory.lesson_stages a, memory.lesson_archived_reasons ar
                      WHERE l.id = d.old_id AND a.code = 'archived' AND ar.code = 'superseded'
                     RETURNING l.id AS old_id
                 ),
                 moved AS (
                     UPDATE memory.lesson_proposal_evidence e
                        SET lesson_id = d.new_id
                       FROM due d
                       JOIN archived ar ON ar.old_id = d.old_id
                      WHERE e.lesson_id = d.old_id
                     RETURNING d.old_id AS old_id
                 ),
                 moved_counts AS (
                     SELECT old_id, count(*)::int AS n FROM moved GROUP BY old_id
                 ),
                 injected AS (
                     SELECT i.lesson_id AS old_id, count(*)::int AS n
                       FROM memory.lesson_injections i
                       JOIN archived ar ON ar.old_id = i.lesson_id
                      GROUP BY i.lesson_id
                 ),
                 inherited AS (
                     INSERT INTO memory.lesson_relations
                            (from_lesson_id, to_lesson_id, kind_id, method_id, status_id, candidate_distance,
                             evidence_json, proposer_model, proposer_rationale, adjudicator_model,
                             adjudicator_agrees, adjudicator_rationale, detector_version, decided_at, decided_by)
                     SELECT CASE WHEN r.from_lesson_id = d.old_id THEN d.new_id ELSE r.from_lesson_id END,
                            CASE WHEN r.to_lesson_id = d.old_id THEN d.new_id ELSE r.to_lesson_id END,
                            r.kind_id, r.method_id, r.status_id, r.candidate_distance,
                            r.evidence_json || jsonb_build_object('inherited_from_lesson', d.old_id),
                            r.proposer_model, r.proposer_rationale, r.adjudicator_model,
                            r.adjudicator_agrees, r.adjudicator_rationale, r.detector_version,
                            r.decided_at, r.decided_by
                       FROM due d
                       JOIN archived ar ON ar.old_id = d.old_id
                       JOIN memory.lesson_relations r
                         ON (r.from_lesson_id = d.old_id OR r.to_lesson_id = d.old_id)
                        AND r.id <> d.relation_id
                      WHERE CASE WHEN r.from_lesson_id = d.old_id THEN d.new_id ELSE r.from_lesson_id END
                         <> CASE WHEN r.to_lesson_id = d.old_id THEN d.new_id ELSE r.to_lesson_id END
                     ON CONFLICT (from_lesson_id, to_lesson_id, kind_id) DO NOTHING
                     RETURNING id
                 ),
                 stamped AS (
                     UPDATE memory.lesson_relations r
                        SET evidence_json = r.evidence_json || jsonb_build_object(
                                'applied_at', now(),
                                'archived_lesson_id', d.old_id,
                                'inherited_injections', COALESCE(inj.n, 0),
                                'moved_evidence', COALESCE(mv.n, 0))
                       FROM due d
                       JOIN archived ar ON ar.old_id = d.old_id
                       LEFT JOIN injected inj ON inj.old_id = d.old_id
                       LEFT JOIN moved_counts mv ON mv.old_id = d.old_id
                      WHERE r.id = d.relation_id
                     RETURNING r.id
                 )
                 SELECT count(*) AS "archived!" FROM archived"#
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(u64::try_from(archived).unwrap_or(0))
        })
        .await
    }
}
