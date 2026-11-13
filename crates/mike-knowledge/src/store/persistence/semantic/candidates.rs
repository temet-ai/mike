use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct NewCandidate {
    pub cluster_version: String,
    pub embedder_key: String,
    pub floor_id: Option<i64>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub member_item_ids: Vec<i64>,
    pub session_count: i32,
    pub medoid_item_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatternCandidateRow {
    pub id: i64,
    pub status: String,
    pub cluster_version: String,
    pub embedder_key: String,
    pub floor_id: Option<i64>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub member_item_ids: Vec<i64>,
    pub member_count: i32,
    pub session_count: i32,
    pub medoid_item_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_note: Option<String>,
    pub named_pattern: Option<String>,
    #[serde(skip)]
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateMemberRow {
    pub candidate_id: i64,
    pub item_id: i64,
    pub ord: i64,
    pub text: Option<String>,
    pub session_id: Option<i64>,
    pub operator_turn_id: Option<i64>,
    pub effective_at: Option<DateTime<Utc>>,
    pub similarity_to_medoid: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CandidateDecision {
    Named {
        sort_order: i32,
        anchor_ids: Vec<i64>,
    },
    Dismissed,
    NotFound,
    NotOpen(String),
    SlugConflict {
        existing_description: String,
    },
}

fn member_array(ids: &[i64]) -> String {
    let parts: Vec<String> = ids.iter().map(i64::to_string).collect();
    format!("{{{}}}", parts.join(","))
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn insert_pattern_candidates(
        self,
        rows: &[NewCandidate],
    ) -> Result<Vec<i64>, PersistError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let versions: Vec<String> = rows.iter().map(|r| r.cluster_version.clone()).collect();
        let keys: Vec<String> = rows.iter().map(|r| r.embedder_key.clone()).collect();
        let floors: Vec<Option<i64>> = rows.iter().map(|r| r.floor_id).collect();
        let starts: Vec<DateTime<Utc>> = rows.iter().map(|r| r.window_start).collect();
        let ends: Vec<DateTime<Utc>> = rows.iter().map(|r| r.window_end).collect();
        let members: Vec<String> = rows
            .iter()
            .map(|r| member_array(&r.member_item_ids))
            .collect();
        let sessions: Vec<i32> = rows.iter().map(|r| r.session_count).collect();
        let medoids: Vec<Option<i64>> = rows.iter().map(|r| r.medoid_item_id).collect();
        crate::metrics::time_db("insert_pattern_candidates", async move {
            Ok(sqlx::query_scalar!(
                r#"INSERT INTO memory.pattern_candidates
                       (cluster_version, embedder_key, floor_id, window_start, window_end,
                        member_item_ids, member_count, session_count, medoid_item_id)
                   SELECT u.version, u.key, u.floor_id, u.ws, u.we, u.members::bigint[],
                          cardinality(u.members::bigint[]), u.sessions, u.medoid
                     FROM unnest($1::text[], $2::text[], $3::bigint[], $4::timestamptz[],
                                 $5::timestamptz[], $6::text[], $7::int[], $8::bigint[])
                          AS u(version, key, floor_id, ws, we, members, sessions, medoid)
                   RETURNING id"#,
                &versions,
                &keys,
                &floors as &[Option<i64>],
                &starts,
                &ends,
                &members,
                &sessions,
                &medoids as &[Option<i64>],
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_pattern_candidates(
        self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<PatternCandidateRow>, Vec<CandidateMemberRow>, i64), PersistError> {
        crate::metrics::time_db("list_pattern_candidates", async move {
            let rows = match status {
                Some(status) => {
                    sqlx::query_as!(
                        PatternCandidateRow,
                        r#"SELECT c.id, c.status, c.cluster_version, c.embedder_key, c.floor_id,
                                  c.window_start, c.window_end, c.member_item_ids, c.member_count,
                                  c.session_count, c.medoid_item_id, c.created_at, c.decided_at,
                                  c.decision_note, c.named_pattern, count(*) OVER () AS "total!"
                             FROM memory.pattern_candidates c
                            WHERE c.status = $1
                            ORDER BY c.created_at DESC, c.id DESC
                            LIMIT $2 OFFSET $3"#,
                        status,
                        limit,
                        offset,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_as!(
                        PatternCandidateRow,
                        r#"SELECT c.id, c.status, c.cluster_version, c.embedder_key, c.floor_id,
                                  c.window_start, c.window_end, c.member_item_ids, c.member_count,
                                  c.session_count, c.medoid_item_id, c.created_at, c.decided_at,
                                  c.decision_note, c.named_pattern, count(*) OVER () AS "total!"
                             FROM memory.pattern_candidates c
                            ORDER BY c.created_at DESC, c.id DESC
                            LIMIT $1 OFFSET $2"#,
                        limit,
                        offset,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            let total = rows.first().map_or(0, |r| r.total);
            let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
            let member_rows = if ids.is_empty() {
                Vec::new()
            } else {
                sqlx::query_file_as!(
                    CandidateMemberRow,
                    "src/store/persistence/semantic/candidate_members.sql",
                    &ids,
                )
                .fetch_all(&self.pool)
                .await?
            };
            Ok((rows, member_rows, total))
        })
        .await
    }

    pub async fn name_pattern_candidate(
        self,
        id: i64,
        slug: &str,
        description: &str,
        builder_version: &str,
    ) -> Result<CandidateDecision, PersistError> {
        crate::metrics::time_db("name_pattern_candidate", async move {
            let mut tx = self.pool.begin().await?;
            let status = lock_candidate_status(&mut tx, id).await?;
            match status.as_deref() {
                None => return Ok(CandidateDecision::NotFound),
                Some("open") => {}
                Some(other) => return Ok(CandidateDecision::NotOpen(other.to_owned())),
            }
            let label = sqlx::query!(
                "WITH next_order AS (
                     SELECT COALESCE(max(sort_order), 0) + 1 AS n
                       FROM eval.axis_labels WHERE axis = 'failure_tag' AND sort_order < 99
                 )
                 INSERT INTO eval.axis_labels (axis, label, subject_kinds, sort_order, description)
                 SELECT 'failure_tag', $1, '{turn,session_end}', n, $2 FROM next_order
                 ON CONFLICT (axis, label) DO SELECT
                 RETURNING sort_order, description",
                slug,
                description,
            )
            .fetch_one(&mut *tx)
            .await?;
            if label.description != description {
                return Ok(CandidateDecision::SlugConflict {
                    existing_description: label.description,
                });
            }
            let sort_order = label.sort_order;
            let anchor_ids = sqlx::query_file_scalar!(
                "src/store/persistence/semantic/candidate_anchors.sql",
                id,
                slug,
                builder_version,
            )
            .fetch_all(&mut *tx)
            .await?;
            sqlx::query!(
                "UPDATE memory.pattern_candidates
                    SET status = 'named', decided_at = now(), named_pattern = $2, decision_note = $3
                  WHERE id = $1",
                id,
                slug,
                description,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(CandidateDecision::Named {
                sort_order,
                anchor_ids,
            })
        })
        .await
    }

    pub async fn dismiss_pattern_candidate(
        self,
        id: i64,
        note: &str,
    ) -> Result<CandidateDecision, PersistError> {
        crate::metrics::time_db("dismiss_pattern_candidate", async move {
            let mut tx = self.pool.begin().await?;
            let status = lock_candidate_status(&mut tx, id).await?;
            match status.as_deref() {
                None => return Ok(CandidateDecision::NotFound),
                Some("open") => {}
                Some(other) => return Ok(CandidateDecision::NotOpen(other.to_owned())),
            }
            sqlx::query!(
                "UPDATE memory.pattern_candidates
                    SET status = 'dismissed', decided_at = now(), decision_note = $2
                  WHERE id = $1",
                id,
                note,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(CandidateDecision::Dismissed)
        })
        .await
    }
}

async fn lock_candidate_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
) -> Result<Option<String>, PersistError> {
    Ok(sqlx::query_scalar!(
        "SELECT status FROM memory.pattern_candidates WHERE id = $1 FOR UPDATE",
        id,
    )
    .fetch_optional(&mut **tx)
    .await?)
}
