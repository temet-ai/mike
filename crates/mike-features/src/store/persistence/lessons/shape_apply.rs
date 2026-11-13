use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sqlx::PgConnection;

use crate::store::persistence::PersistError;
use crate::store::persistence::operator_turns::vector_to_text_literal;

pub const SHAPE_DETECTOR_VERSION: &str = "lesson-shape-v1";

#[derive(Debug, Clone, PartialEq)]
pub struct RuleEmbedding {
    pub vector: Vec<f32>,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RewrittenRule {
    pub text: String,
    pub embedding: Option<RuleEmbedding>,
}

pub struct ShapeRewrite<'a> {
    pub lesson_id: i64,
    pub rules: &'a [RewrittenRule],
    pub model: &'a str,
    pub original_chars: usize,
    pub rationale: &'a str,
}

fn sha_hex(text: &str) -> String {
    Sha256::digest(text.trim().to_lowercase().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn rules_payload(rules: &[RewrittenRule]) -> Value {
    Value::Array(
        rules
            .iter()
            .map(|rule| {
                json!({
                    "text": rule.text,
                    "sha": sha_hex(&rule.text),
                    "embedding": rule.embedding.as_ref().map(|e| vector_to_text_literal(&e.vector)),
                    "model": rule.embedding.as_ref().map(|e| e.model.as_str()),
                })
            })
            .collect(),
    )
}

async fn lock_active(conn: &mut PgConnection, lesson_id: i64) -> Result<bool, sqlx::Error> {
    let locked = sqlx::query_scalar!(
        r#"SELECT l.id AS "id!" FROM memory.lessons l
            WHERE l.id = $1 AND l.archived_at IS NULL FOR UPDATE"#,
        lesson_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(locked.is_some())
}

async fn insert_replacements(
    conn: &mut PgConnection,
    lesson_id: i64,
    payload: Value,
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"INSERT INTO memory.lessons
                (project_id, kind_id, text, text_sha256, topic, stage_id, source_turn_id,
                 source_id, agent_id, agent_instance_id, session_id, request_id,
                 created_at, expires_at, embedding, embedding_model, embedded_at)
           SELECT o.project_id, o.kind_id, r.text, decode(r.sha, 'hex'), o.topic,
                  o.stage_id, o.source_turn_id, o.source_id, o.agent_id,
                  o.agent_instance_id, o.session_id, o.request_id,
                  o.created_at, o.expires_at,
                  r.embedding::vector, r.model,
                  CASE WHEN r.embedding IS NULL THEN NULL ELSE now() END
             FROM memory.lessons o
            CROSS JOIN jsonb_to_recordset($2::jsonb)
                  AS r(text text, sha text, embedding text, model text)
            WHERE o.id = $1
        RETURNING id AS "id!""#,
        lesson_id,
        payload,
    )
    .fetch_all(conn)
    .await
}

async fn record_replacements(
    conn: &mut PgConnection,
    ids: &[i64],
    rewrite: &ShapeRewrite<'_>,
) -> Result<(), sqlx::Error> {
    let original_chars = i32::try_from(rewrite.original_chars).unwrap_or(i32::MAX);
    sqlx::query!(
        "INSERT INTO memory.lesson_shape_findings
            (lesson_id, shape, detail, chars, resolution, resolved_at)
         SELECT l.id, 'rule', 'rewritten from lesson ' || $2::text, char_length(l.text),
                'none_needed', now()
           FROM memory.lessons l
          WHERE l.id = ANY($1)
         ON CONFLICT (lesson_id) DO NOTHING",
        ids,
        rewrite.lesson_id.to_string(),
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "INSERT INTO memory.lesson_relations
            (from_lesson_id, to_lesson_id, kind_id, method_id, status_id, evidence_json,
             proposer_model, proposer_rationale, detector_version, decided_at, decided_by)
         SELECT n.id, $2, rk.id, rm.id, rs.id,
                jsonb_build_object('original_chars', $4::int, 'rule_count', cardinality($1::bigint[])),
                $3, $5, $6, now(), 'shape_pass'
           FROM UNNEST($1::bigint[]) AS n(id),
                memory.lesson_relation_kinds rk,
                memory.lesson_relation_methods rm,
                memory.lesson_relation_statuses rs
          WHERE rk.code = 'supersedes' AND rm.code = 'rewritten' AND rs.code = 'confirmed'",
        ids,
        rewrite.lesson_id,
        rewrite.model,
        original_chars,
        rewrite.rationale,
        SHAPE_DETECTOR_VERSION,
    )
    .execute(conn)
    .await?;
    Ok(())
}

async fn archive_lesson(
    conn: &mut PgConnection,
    lesson_id: i64,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE memory.lessons l
            SET stage_id = archived_s.id, archived_at = now(), archived_reason_id = ar.id
           FROM memory.lesson_stages archived_s, memory.lesson_archived_reasons ar
          WHERE archived_s.code = 'archived' AND ar.code = $2
            AND l.id = $1 AND l.archived_at IS NULL",
        lesson_id,
        reason,
    )
    .execute(conn)
    .await?
    .rows_affected()
        > 0)
}

async fn resolve_finding(
    conn: &mut PgConnection,
    lesson_id: i64,
    resolution: &str,
    replacement: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE memory.lesson_shape_findings
            SET resolution = $2, resolved_at = now(), replacement_lesson_id = $3
          WHERE lesson_id = $1 AND resolution IS NULL",
        lesson_id,
        resolution,
        replacement,
    )
    .execute(conn)
    .await?;
    Ok(())
}

async fn move_evidence(
    conn: &mut PgConnection,
    from_lesson_id: i64,
    to_lesson_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE memory.lesson_proposal_evidence
            SET lesson_id = $2
          WHERE lesson_id = $1",
        from_lesson_id,
        to_lesson_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn rewrite_lesson_into_rules(
        self,
        rewrite: &ShapeRewrite<'_>,
    ) -> Result<Option<Vec<i64>>, PersistError> {
        let payload = rules_payload(rewrite.rules);
        crate::metrics::time_db("rewrite_lesson_into_rules", async move {
            let mut tx = self.pool.begin().await?;
            if !lock_active(&mut tx, rewrite.lesson_id).await? {
                return Ok(None);
            }
            let ids = insert_replacements(&mut tx, rewrite.lesson_id, payload).await?;
            record_replacements(&mut tx, &ids, rewrite).await?;
            move_evidence(&mut tx, rewrite.lesson_id, ids.first().copied()).await?;
            archive_lesson(&mut tx, rewrite.lesson_id, "superseded").await?;
            resolve_finding(
                &mut tx,
                rewrite.lesson_id,
                "rewritten",
                ids.first().copied(),
            )
            .await?;
            tx.commit().await?;
            Ok(Some(ids))
        })
        .await
    }

    pub async fn retire_lesson_as_not_a_lesson(self, lesson_id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("retire_lesson_as_not_a_lesson", async move {
            let mut tx = self.pool.begin().await?;
            let archived = archive_lesson(&mut tx, lesson_id, "not_a_lesson").await?;
            if archived {
                resolve_finding(&mut tx, lesson_id, "not_a_lesson", None).await?;
            }
            tx.commit().await?;
            Ok(archived)
        })
        .await
    }
}
