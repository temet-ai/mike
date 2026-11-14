use sqlx::PgConnection;

use crate::lesson_shape::Assessment;
use crate::store::persistence::PersistError;

pub const MAX_SHAPE_ATTEMPTS: i32 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct UncheckedLesson {
    pub id: i64,
    pub text: String,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeWork {
    pub lesson_id: i64,
    pub text: String,
    pub topic: Option<String>,
    pub shape: String,
    pub detail: String,
    pub attempts: i32,
    pub kind: String,
    pub stage: String,
    pub source: String,
    pub project_id: Option<i64>,
    pub session_id: Option<i64>,
    pub session_uuid: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeFindingRow {
    pub lesson_id: i64,
    pub shape: String,
    pub detail: String,
    pub chars: i32,
    pub attempts: i32,
    pub resolution: Option<String>,
    pub replacement_lesson_id: Option<i64>,
}

fn chars_column(chars: usize) -> i32 {
    i32::try_from(chars).unwrap_or(i32::MAX)
}

pub async fn insert_shape_finding(
    conn: &mut PgConnection,
    lesson_id: i64,
    assessment: &Assessment,
    hold: bool,
) -> Result<(), PersistError> {
    sqlx::query!(
        "INSERT INTO memory.lesson_shape_findings
            (lesson_id, shape, detail, chars, resolution, resolved_at)
         VALUES ($1, $2, $3, $4,
                 CASE WHEN $2 = 'rule' THEN 'none_needed' WHEN NOT $5 THEN 'kept' END,
                 CASE WHEN $2 = 'rule' OR NOT $5 THEN now() END)
         ON CONFLICT (lesson_id) DO NOTHING",
        lesson_id,
        assessment.shape.code(),
        assessment.detail,
        chars_column(assessment.chars),
        hold,
    )
    .execute(conn)
    .await?;
    Ok(())
}

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn unchecked_lessons_for_shape(
        self,
        limit: i64,
    ) -> Result<Vec<UncheckedLesson>, PersistError> {
        crate::metrics::time_db("unchecked_lessons_for_shape", async move {
            Ok(sqlx::query_as!(
                UncheckedLesson,
                "SELECT l.id, l.text, l.topic
                   FROM memory.lessons l
                  WHERE l.archived_at IS NULL
                    AND NOT EXISTS (SELECT 1 FROM memory.lesson_shape_findings f WHERE f.lesson_id = l.id)
                  ORDER BY l.id
                  LIMIT $1",
                limit
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn record_shape_findings(
        self,
        findings: &[(i64, Assessment)],
    ) -> Result<u64, PersistError> {
        if findings.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = findings.iter().map(|(id, _)| *id).collect();
        let shapes: Vec<String> = findings
            .iter()
            .map(|(_, a)| a.shape.code().to_owned())
            .collect();
        let details: Vec<String> = findings.iter().map(|(_, a)| a.detail.clone()).collect();
        let chars: Vec<i32> = findings
            .iter()
            .map(|(_, a)| chars_column(a.chars))
            .collect();
        crate::metrics::time_db("record_shape_findings", async move {
            let done = sqlx::query!(
                "INSERT INTO memory.lesson_shape_findings
                    (lesson_id, shape, detail, chars, resolution, resolved_at)
                 SELECT u.id, u.shape, u.detail, u.chars,
                        CASE WHEN u.shape = 'rule' THEN 'none_needed' END,
                        CASE WHEN u.shape = 'rule' THEN now() END
                   FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::int[])
                        AS u(id, shape, detail, chars)
                 ON CONFLICT (lesson_id) DO NOTHING",
                &ids[..],
                &shapes[..],
                &details[..],
                &chars[..],
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn claim_shape_work(self, limit: i64) -> Result<Vec<ShapeWork>, PersistError> {
        crate::metrics::time_db("claim_shape_work", async move {
            Ok(sqlx::query_as!(
                ShapeWork,
                r#"SELECT f.lesson_id, l.text, l.topic, f.shape, f.detail, f.attempts,
                        lk.code AS kind, ls.code AS stage, src.code AS source,
                        l.project_id, l.session_id, s.session_uuid AS "session_uuid?"
                   FROM memory.lesson_shape_findings f
                   JOIN memory.lessons l ON l.id = f.lesson_id AND l.archived_at IS NULL
                   JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                   JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                   JOIN memory.lesson_sources src ON src.id = l.source_id
                   LEFT JOIN sessions.sessions s ON s.id = l.session_id
                  WHERE f.resolution IS NULL
                    AND f.attempts < $2
                    AND (f.last_attempt_at IS NULL
                         OR f.last_attempt_at <= now() - interval '1 hour' * (2 ^ f.attempts))
                  ORDER BY (ls.code = 'raw') DESC, f.found_at, f.lesson_id
                  LIMIT $1"#,
                limit,
                MAX_SHAPE_ATTEMPTS,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn record_shape_failure(
        self,
        lesson_id: i64,
        error: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("record_shape_failure", async move {
            let kept = sqlx::query_scalar!(
                r#"UPDATE memory.lesson_shape_findings
                      SET attempts = attempts + 1,
                          last_error = $2,
                          last_attempt_at = now(),
                          resolution = CASE WHEN attempts + 1 >= $3 THEN 'kept' END,
                          resolved_at = CASE WHEN attempts + 1 >= $3 THEN now() END
                    WHERE lesson_id = $1 AND resolution IS NULL
                RETURNING (resolution IS NOT NULL) AS "kept!""#,
                lesson_id,
                error,
                MAX_SHAPE_ATTEMPTS,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(kept.unwrap_or(false))
        })
        .await
    }

    pub async fn release_stale_shape_findings(self, after_days: i32) -> Result<u64, PersistError> {
        crate::metrics::time_db("release_stale_shape_findings", async move {
            Ok(sqlx::query!(
                "UPDATE memory.lesson_shape_findings
                    SET resolution = 'kept',
                        resolved_at = now(),
                        last_error = COALESCE(last_error, 'released: no rewrite within ' || $1::integer::text || ' days')
                  WHERE resolution IS NULL
                    AND found_at < now() - make_interval(days => $1)",
                after_days,
            )
            .execute(&self.pool)
            .await?
            .rows_affected())
        })
        .await
    }

    pub async fn lesson_has_open_shape_finding(self, lesson_id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("lesson_has_open_shape_finding", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT EXISTS (
                       SELECT 1 FROM memory.lesson_shape_findings f
                        WHERE f.lesson_id = $1 AND f.resolution IS NULL) AS "open!""#,
                lesson_id,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn shape_finding(
        self,
        lesson_id: i64,
    ) -> Result<Option<ShapeFindingRow>, PersistError> {
        crate::metrics::time_db("shape_finding", async move {
            Ok(sqlx::query_as!(
                ShapeFindingRow,
                "SELECT lesson_id, shape, detail, chars, attempts, resolution,
                        replacement_lesson_id
                   FROM memory.lesson_shape_findings
                  WHERE lesson_id = $1",
                lesson_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }
}
