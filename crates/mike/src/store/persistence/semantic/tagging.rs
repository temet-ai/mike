use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

const ASSIGNMENT_SELECT: &str =
    "SELECT a.id, a.corrected_turn_id, a.correction_item_id, i.text AS correction_text,
        a.qualification, a.qualification_ref, a.pattern, a.score, a.similarity, a.anchor_item_id,
        a.runner_up_pattern, a.margin, a.top_anchors, a.floor_id, a.tagger_version, a.embedder_key,
        a.assigned_at, count(*) OVER () AS total
   FROM memory.pattern_assignments a
   JOIN memory.semantic_items i ON i.id = a.correction_item_id
  WHERE true";

#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionForTagging {
    pub item_id: i64,
    pub corrected_turn_id: i64,
    pub session_uuid: String,
    pub request_id: Option<i64>,
    pub qualification: String,
    pub qualification_ref: String,
    pub embedding: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnchorCandidateRow {
    pub correction_item_id: i64,
    pub anchor_item_id: i64,
    pub pattern: Option<String>,
    pub origin_session_uuid: Option<String>,
    pub request_id: Option<i64>,
    pub similarity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAssignment {
    pub corrected_turn_id: i64,
    pub correction_item_id: i64,
    pub qualification: String,
    pub qualification_ref: String,
    pub pattern: Option<String>,
    pub score: f64,
    pub similarity: f64,
    pub anchor_item_id: Option<i64>,
    pub runner_up_pattern: Option<String>,
    pub margin: Option<f64>,
    pub top_anchors: serde_json::Value,
    pub floor_id: i64,
    pub tagger_version: String,
    pub embedder_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct AssignmentRow {
    pub id: i64,
    pub corrected_turn_id: i64,
    pub correction_item_id: i64,
    pub correction_text: String,
    pub qualification: String,
    pub qualification_ref: String,
    pub pattern: Option<String>,
    pub score: f64,
    pub similarity: f64,
    pub anchor_item_id: Option<i64>,
    pub runner_up_pattern: Option<String>,
    pub margin: Option<f64>,
    pub top_anchors: serde_json::Value,
    pub floor_id: i64,
    pub tagger_version: String,
    pub embedder_key: String,
    pub assigned_at: DateTime<Utc>,
    #[serde(skip)]
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResidueRow {
    pub item_id: i64,
    pub session_id: Option<i64>,
    pub embedding: String,
}

pub struct AssignmentFilter<'a> {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub pattern: Option<&'a str>,
    pub unassigned: bool,
    pub limit: i64,
    pub offset: i64,
}

impl PersistHandle {
    pub async fn corrections_needing_tagging(
        &self,
        embedder_key: &str,
        tagger_version: &str,
        limit: i64,
    ) -> Result<Vec<CorrectionForTagging>, PersistError> {
        crate::metrics::time_db("corrections_needing_tagging", async move {
            Ok(sqlx::query_file_as!(
                CorrectionForTagging,
                "src/store/persistence/semantic/tagging_corrections.sql",
                embedder_key,
                tagger_version,
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn nearest_anchors(
        &self,
        corrections: &[CorrectionForTagging],
        embedder_key: &str,
    ) -> Result<Vec<AnchorCandidateRow>, PersistError> {
        if corrections.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = corrections.iter().map(|c| c.item_id).collect();
        let sessions: Vec<String> = corrections.iter().map(|c| c.session_uuid.clone()).collect();
        let requests: Vec<Option<i64>> = corrections.iter().map(|c| c.request_id).collect();
        let vecs: Vec<String> = corrections.iter().map(|c| c.embedding.clone()).collect();
        crate::metrics::time_db("nearest_anchors", async move {
            Ok(sqlx::query_file_as!(
                AnchorCandidateRow,
                "src/store/persistence/semantic/tagging_anchors.sql",
                &ids,
                &sessions[..],
                &requests as &[Option<i64>],
                &vecs[..],
                embedder_key,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn insert_pattern_assignments(
        &self,
        rows: &[NewAssignment],
    ) -> Result<u64, PersistError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let corrected_turns: Vec<i64> = rows.iter().map(|r| r.corrected_turn_id).collect();
        let correction_items: Vec<i64> = rows.iter().map(|r| r.correction_item_id).collect();
        let qualifications: Vec<String> = rows.iter().map(|r| r.qualification.clone()).collect();
        let qualification_refs: Vec<String> =
            rows.iter().map(|r| r.qualification_ref.clone()).collect();
        let patterns: Vec<Option<String>> = rows.iter().map(|r| r.pattern.clone()).collect();
        let scores: Vec<f64> = rows.iter().map(|r| r.score).collect();
        let similarities: Vec<f64> = rows.iter().map(|r| r.similarity).collect();
        let anchors: Vec<Option<i64>> = rows.iter().map(|r| r.anchor_item_id).collect();
        let runners_up: Vec<Option<String>> =
            rows.iter().map(|r| r.runner_up_pattern.clone()).collect();
        let margins: Vec<Option<f64>> = rows.iter().map(|r| r.margin).collect();
        let top_anchors: Vec<serde_json::Value> =
            rows.iter().map(|r| r.top_anchors.clone()).collect();
        let floors: Vec<i64> = rows.iter().map(|r| r.floor_id).collect();
        let tagger_versions: Vec<String> = rows.iter().map(|r| r.tagger_version.clone()).collect();
        let embedder_keys: Vec<String> = rows.iter().map(|r| r.embedder_key.clone()).collect();
        crate::metrics::time_db("insert_pattern_assignments", async move {
            let r = sqlx::query!(
                "INSERT INTO memory.pattern_assignments
                     (corrected_turn_id, correction_item_id, qualification, qualification_ref,
                      pattern, score, similarity, anchor_item_id, runner_up_pattern, margin,
                      top_anchors, floor_id, tagger_version, embedder_key)
                 SELECT u.corrected_turn_id, u.correction_item_id, u.qualification,
                        u.qualification_ref, u.pattern, u.score, u.similarity, u.anchor_item_id,
                        u.runner_up_pattern, u.margin, u.top_anchors, u.floor_id,
                        u.tagger_version, u.embedder_key
                   FROM unnest($1::bigint[], $2::bigint[], $3::text[], $4::text[], $5::text[],
                               $6::float8[], $7::float8[], $8::bigint[], $9::text[],
                               $10::float8[], $11::jsonb[], $12::bigint[], $13::text[],
                               $14::text[])
                        AS u(corrected_turn_id, correction_item_id, qualification,
                             qualification_ref, pattern, score, similarity, anchor_item_id,
                             runner_up_pattern, margin, top_anchors, floor_id, tagger_version,
                             embedder_key)
                 ON CONFLICT (correction_item_id, tagger_version, embedder_key) DO NOTHING",
                &corrected_turns,
                &correction_items,
                &qualifications[..],
                &qualification_refs[..],
                &patterns as &[Option<String>],
                &scores,
                &similarities,
                &anchors as &[Option<i64>],
                &runners_up as &[Option<String>],
                &margins as &[Option<f64>],
                &top_anchors,
                &floors,
                &tagger_versions[..],
                &embedder_keys[..],
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected())
        })
        .await
    }

    pub async fn list_pattern_assignments(
        &self,
        f: &AssignmentFilter<'_>,
    ) -> Result<(Vec<AssignmentRow>, i64), PersistError> {
        crate::metrics::time_db("list_pattern_assignments", async move {
            let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(ASSIGNMENT_SELECT);
            if let Some(from) = f.from {
                query.push(" AND a.assigned_at >= ").push_bind(from);
            }
            if let Some(to) = f.to {
                query.push(" AND a.assigned_at < ").push_bind(to);
            }
            if let Some(pattern) = f.pattern {
                query.push(" AND a.pattern = ").push_bind(pattern);
            }
            if f.unassigned {
                query.push(" AND a.pattern IS NULL");
            }
            query.push(" ORDER BY a.assigned_at DESC, a.id DESC LIMIT ");
            query.push_bind(f.limit);
            query.push(" OFFSET ").push_bind(f.offset);
            let rows = query
                .build_query_as::<AssignmentRow>()
                .fetch_all(&self.pool)
                .await?;
            let total = rows.first().map_or(0, |r| r.total);
            Ok((rows, total))
        })
        .await
    }

    pub async fn pattern_residue(
        &self,
        embedder_key: &str,
    ) -> Result<Vec<ResidueRow>, PersistError> {
        crate::metrics::time_db("build_candidate_graph", async move {
            Ok(sqlx::query_as!(
                ResidueRow,
                r#"WITH blocked AS (
                       SELECT unnest(c.member_item_ids) AS item_id
                         FROM memory.pattern_candidates c
                        WHERE c.status = 'open'
                           OR (c.status = 'dismissed' AND c.decided_at > now() - interval '30 days')
                   )
                   SELECT i.id AS item_id, i.session_id, i.embedding::text AS "embedding!"
                     FROM memory.pattern_assignments a
                     JOIN memory.semantic_items i ON i.id = a.correction_item_id
                    WHERE a.pattern IS NULL AND a.score < 0 AND a.embedder_key = $1
                      AND i.effective_at > now() - interval '30 days'
                      AND NOT EXISTS (SELECT 1 FROM blocked b WHERE b.item_id = i.id)
                    ORDER BY i.id"#,
                embedder_key,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
