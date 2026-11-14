use super::PersistError;
use sqlx::{PgPool, QueryBuilder};

pub struct LessonRelationDraft<'a> {
    pub from_lesson_id: i64,
    pub to_lesson_id: i64,
    pub kind: &'a str,
    pub method: &'a str,
    pub status: &'a str,
    pub candidate_distance: Option<f64>,
    pub evidence_json: serde_json::Value,
    pub proposer_model: Option<&'a str>,
    pub proposer_rationale: Option<&'a str>,
    pub adjudicator_model: Option<&'a str>,
    pub adjudicator_agrees: Option<bool>,
    pub adjudicator_rationale: Option<&'a str>,
    pub detector_version: &'a str,
    pub decided_by: Option<&'a str>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct LessonRelationRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub from_lesson_id: i64,
    pub from_lesson_text: String,
    pub from_lesson_created_at: chrono::DateTime<chrono::Utc>,
    pub to_lesson_id: i64,
    pub to_lesson_text: String,
    pub to_lesson_created_at: chrono::DateTime<chrono::Utc>,
    pub kind: String,
    pub method: String,
    pub status: String,
    pub candidate_distance: Option<f64>,
    pub evidence_json: serde_json::Value,
    pub proposer_model: Option<String>,
    pub proposer_rationale: Option<String>,
    pub adjudicator_model: Option<String>,
    pub adjudicator_agrees: Option<bool>,
    pub adjudicator_rationale: Option<String>,
    pub detector_version: String,
    pub proposed_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const RELATION_SELECT: &str = "r.id, r.uuid,
     r.from_lesson_id, lf.text AS from_lesson_text, lf.created_at AS from_lesson_created_at,
     r.to_lesson_id, lt.text AS to_lesson_text, lt.created_at AS to_lesson_created_at,
     rk.code AS kind, rm.code AS method, rs.code AS status,
     r.candidate_distance, r.evidence_json,
     r.proposer_model, r.proposer_rationale,
     r.adjudicator_model, r.adjudicator_agrees, r.adjudicator_rationale,
     r.detector_version, r.proposed_at, r.decided_at, r.decided_by, r.created_at";

const RELATION_FROM: &str = "memory.lesson_relations r
     JOIN memory.lessons lf ON lf.id = r.from_lesson_id
     JOIN memory.lessons lt ON lt.id = r.to_lesson_id
     JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id
     JOIN memory.lesson_relation_methods rm ON rm.id = r.method_id
     JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id";

#[derive(Debug, Clone)]
pub struct CandidateLessonPair {
    pub lesson_a_id: i64,
    pub lesson_a_text: String,
    pub lesson_a_created_at: chrono::DateTime<chrono::Utc>,
    pub lesson_b_id: i64,
    pub lesson_b_text: String,
    pub lesson_b_created_at: chrono::DateTime<chrono::Utc>,
    pub distance: f64,
}

#[derive(Debug, Clone, Default)]
pub struct LessonRelationsFilter {
    pub project: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug)]
pub enum RelationDecisionOutcome {
    NotFound,
    WrongStatus(String),
    Decided(Box<LessonRelationRow>),
}

fn push_relation_filters(
    builder: &mut QueryBuilder<sqlx::Postgres>,
    filter: &LessonRelationsFilter,
) {
    builder.push(" WHERE true");
    if let Some(project) = &filter.project {
        builder.push(" AND (pf.project_path = ");
        builder.push_bind(project.clone());
        builder.push(" OR pt.project_path = ");
        builder.push_bind(project.clone());
        builder.push(")");
    }
    if let Some(kind) = &filter.kind {
        builder.push(" AND rk.code = ");
        builder.push_bind(kind.clone());
    }
    if let Some(status) = &filter.status {
        builder.push(" AND rs.code = ");
        builder.push_bind(status.clone());
    }
}

const PROJECT_JOINS: &str = " LEFT JOIN projects.projects pf ON pf.id = lf.project_id
     LEFT JOIN projects.projects pt ON pt.id = lt.project_id";

async fn count_relations(
    pool: &PgPool,
    filter: &LessonRelationsFilter,
) -> Result<i64, PersistError> {
    let mut builder = QueryBuilder::new(
        "SELECT count(*)
           FROM memory.lesson_relations r
           JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id
           JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id",
    );
    if filter.project.is_some() {
        builder.push(
            " JOIN memory.lessons lf ON lf.id = r.from_lesson_id
              JOIN memory.lessons lt ON lt.id = r.to_lesson_id",
        );
        builder.push(PROJECT_JOINS);
    }
    push_relation_filters(&mut builder, filter);
    let total: i64 = builder.build_query_scalar().fetch_one(pool).await?;
    Ok(total)
}

async fn page_relations(
    pool: &PgPool,
    filter: &LessonRelationsFilter,
) -> Result<Vec<LessonRelationRow>, PersistError> {
    let mut builder = QueryBuilder::new("SELECT ");
    builder.push(RELATION_SELECT);
    builder.push(" FROM ");
    builder.push(RELATION_FROM);
    if filter.project.is_some() {
        builder.push(PROJECT_JOINS);
    }
    push_relation_filters(&mut builder, filter);
    builder.push(" ORDER BY r.proposed_at DESC OFFSET ");
    builder.push_bind(filter.offset);
    builder.push(" LIMIT ");
    builder.push_bind(filter.limit);
    let rows = builder
        .build_query_as::<LessonRelationRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn insert_lesson_relation(
        self,
        draft: &LessonRelationDraft<'_>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_lesson_relation", async move {
            let id = sqlx::query_scalar!(
                "INSERT INTO memory.lesson_relations
                    (from_lesson_id, to_lesson_id, kind_id, method_id, status_id,
                     candidate_distance, evidence_json,
                     proposer_model, proposer_rationale,
                     adjudicator_model, adjudicator_agrees, adjudicator_rationale,
                     detector_version, decided_at, decided_by)
                 SELECT $1::bigint, $2::bigint, rk.id, rm.id, rs.id,
                        $6::float8, $7::jsonb, $8::text, $9::text, $10::text, $11::boolean, $12::text,
                        $13::text,
                        CASE WHEN $14::text IS NOT NULL THEN NOW() ELSE NULL END, $14::text
                   FROM memory.lesson_relation_kinds rk,
                        memory.lesson_relation_methods rm,
                        memory.lesson_relation_statuses rs
                  WHERE rk.code = $3 AND rm.code = $4 AND rs.code = $5
              RETURNING id",
                draft.from_lesson_id,
                draft.to_lesson_id,
                draft.kind,
                draft.method,
                draft.status,
                draft.candidate_distance,
                draft.evidence_json,
                draft.proposer_model,
                draft.proposer_rationale,
                draft.adjudicator_model,
                draft.adjudicator_agrees,
                draft.adjudicator_rationale,
                draft.detector_version,
                draft.decided_by,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn live_replacements_of(self, lesson_id: i64) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("live_replacements_of", async move {
            let ids = sqlx::query_scalar!(
                r#"WITH RECURSIVE chain AS (
                    SELECT $1::bigint AS id, 0 AS depth
                    UNION ALL
                    SELECT r.from_lesson_id, c.depth + 1
                      FROM chain c
                      JOIN memory.lesson_relations r ON r.to_lesson_id = c.id
                      JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id AND rk.code = 'supersedes'
                      JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id AND rs.code = 'confirmed'
                     WHERE c.depth < 12
                 )
                 SELECT DISTINCT c.id AS "id!"
                   FROM chain c
                   JOIN memory.lessons l ON l.id = c.id
                  WHERE c.id <> $1 AND l.archived_at IS NULL
                  ORDER BY c.id"#,
                lesson_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }

    pub async fn lesson_exists(self, id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("lesson_relations_lesson_exists", async move {
            let exists = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM memory.lessons WHERE id = $1) AS "exists!""#,
                id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(exists)
        })
        .await
    }

    pub async fn confirm_relation(
        self,
        id: i64,
        decided_by: &str,
    ) -> Result<RelationDecisionOutcome, PersistError> {
        self.decide_relation(id, decided_by, "confirmed").await
    }

    pub async fn reject_relation(
        self,
        id: i64,
        decided_by: &str,
    ) -> Result<RelationDecisionOutcome, PersistError> {
        self.decide_relation(id, decided_by, "rejected").await
    }

    async fn decide_relation(
        self,
        id: i64,
        decided_by: &str,
        target_status: &str,
    ) -> Result<RelationDecisionOutcome, PersistError> {
        let decided_by = decided_by.to_owned();
        let target_status = target_status.to_owned();
        crate::metrics::time_db("decide_relation", async move {
            let updated_id = sqlx::query_scalar!(
                r#"UPDATE memory.lesson_relations r
                      SET status_id  = target_s.id,
                          decided_at = NOW(),
                          decided_by = $2
                     FROM memory.lesson_relation_statuses target_s,
                          memory.lesson_relation_statuses current_s
                    WHERE target_s.code = $3
                      AND r.id = $1
                      AND r.status_id = current_s.id
                      AND current_s.code IN ('proposed', 'contested')
                RETURNING r.id AS "id!""#,
                id,
                decided_by,
                target_status,
            )
            .fetch_optional(&self.pool)
            .await?;

            let Some(updated_id) = updated_id else {
                let current_status = sqlx::query_scalar!(
                    "SELECT rs.code
                       FROM memory.lesson_relations r
                       JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id
                      WHERE r.id = $1",
                    id,
                )
                .fetch_optional(&self.pool)
                .await?;
                return Ok(match current_status {
                    Some(status) => RelationDecisionOutcome::WrongStatus(status),
                    None => RelationDecisionOutcome::NotFound,
                });
            };

            let row = sqlx::query_file_as!(
                LessonRelationRow,
                "src/store/persistence/lesson_relations/relations_by_id.sql",
                updated_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(RelationDecisionOutcome::Decided(Box::new(row)))
        })
        .await
    }

    pub async fn list_relations(
        self,
        filter: &LessonRelationsFilter,
    ) -> Result<(i64, Vec<LessonRelationRow>), PersistError> {
        let filter = filter.clone();
        crate::metrics::time_db("list_relations", async move {
            let total_count = count_relations(&self.pool, &filter).await?;
            let rows = page_relations(&self.pool, &filter).await?;
            Ok((total_count, rows))
        })
        .await
    }

    pub async fn list_relations_for_lesson(
        self,
        lesson_id: i64,
    ) -> Result<Vec<LessonRelationRow>, PersistError> {
        crate::metrics::time_db("list_relations_for_lesson", async move {
            let rows = sqlx::query_file_as!(
                LessonRelationRow,
                "src/store/persistence/lesson_relations/relations_for_lesson.sql",
                lesson_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn lookup_project_id_by_path(
        self,
        project_path: &str,
    ) -> Result<Option<i64>, PersistError> {
        let project_path = project_path.to_owned();
        crate::metrics::time_db("lesson_relations_lookup_project_id_by_path", async move {
            let id = sqlx::query_scalar!(
                "SELECT id FROM projects.projects WHERE project_path = $1",
                project_path,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn list_project_ids_with_embedded_lessons(self) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("list_project_ids_with_embedded_lessons", async move {
            let ids = sqlx::query_scalar!(
                r#"SELECT DISTINCT l.project_id AS "project_id!"
                     FROM memory.lessons l
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                    WHERE l.project_id IS NOT NULL AND l.embedding IS NOT NULL
                    ORDER BY l.project_id"#
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }

    pub async fn release_stale_adjudications(
        self,
        project_id: i64,
        detector_version: &str,
    ) -> Result<u64, PersistError> {
        let detector_version = detector_version.to_owned();
        crate::metrics::time_db("release_stale_adjudications", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/lesson_relations/release_stale_adjudications.sql",
                project_id,
                detector_version
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn find_candidate_relation_pairs(
        self,
        project_id: i64,
        distance_threshold: f64,
    ) -> Result<Vec<CandidateLessonPair>, PersistError> {
        crate::metrics::time_db("find_candidate_relation_pairs", async move {
            let rows = sqlx::query_file_as!(
                CandidateLessonPair,
                "src/store/persistence/lesson_relations/candidate_pairs.sql",
                project_id,
                distance_threshold
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_injectable_lessons(
        self,
        project_path: &str,
    ) -> Result<Vec<super::lessons::LessonRow>, PersistError> {
        let project_path = project_path.to_owned();
        crate::metrics::time_db("list_injectable_lessons", async move {
            let rows = sqlx::query_as::<_, super::lessons::LessonRow>( // grader-allow: reads memory.lessons.retired_at, added in 20261111170700, before its .sqlx entry is prepared
                r#"SELECT l.id, l.project_id, p.project_path, lk.code AS kind, l.text, l.agent_id,
                          ls.code AS stage, l.source_turn_id, l.created_at, l.expires_at, l.topic
                     FROM memory.lessons l
                     LEFT JOIN projects.projects p ON p.id = l.project_id
                     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                     JOIN memory.lesson_sources src ON src.id = l.source_id
                     LEFT JOIN memory.lesson_shape_findings open_shape
                       ON open_shape.lesson_id = l.id AND open_shape.resolution IS NULL
                    WHERE p.project_path = $1
                      AND (ls.code IN ('daily', 'long_term') OR (ls.code = 'raw' AND src.code = 'operator'))
                      AND (l.expires_at IS NULL OR l.expires_at > NOW())
                      AND l.retired_at IS NULL
                      AND NOT EXISTS (
                          SELECT 1
                            FROM memory.lesson_relations r
                            JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id AND rk.code = 'supersedes'
                            JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id AND rs.code = 'confirmed'
                           WHERE r.to_lesson_id = l.id
                      )
                      AND (ls.code <> 'raw' OR open_shape.lesson_id IS NULL)
                    ORDER BY l.created_at DESC"#,
            )
            .bind(project_path)
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
