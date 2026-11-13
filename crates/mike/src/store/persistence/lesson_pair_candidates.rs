use super::PersistError;
use super::lesson_relations::CandidateLessonPair;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NewPairCandidate {
    pub(crate) lesson_a_id: i64,
    pub(crate) lesson_b_id: i64,
    pub(crate) score: f32,
    pub(crate) distance: f64,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub(crate) async fn upsert_lesson_pair_candidates(
        self,
        pairs: &[NewPairCandidate],
    ) -> Result<usize, PersistError> {
        if pairs.is_empty() {
            return Ok(0);
        }
        let a: Vec<i64> = pairs.iter().map(|p| p.lesson_a_id).collect();
        let b: Vec<i64> = pairs.iter().map(|p| p.lesson_b_id).collect();
        let scores: Vec<f32> = pairs.iter().map(|p| p.score).collect();
        let distances: Vec<f64> = pairs.iter().map(|p| p.distance).collect();
        crate::metrics::time_db("upsert_lesson_pair_candidates", async move {
            let written = sqlx::query!(
                r#"INSERT INTO memory.lesson_pair_candidates (lesson_a_id, lesson_b_id, score, distance)
                   SELECT u.lesson_a_id, u.lesson_b_id, u.score, u.distance
                     FROM UNNEST($1::bigint[], $2::bigint[], $3::real[], $4::float8[])
                          AS u(lesson_a_id, lesson_b_id, score, distance)
                   ON CONFLICT (lesson_a_id, lesson_b_id) DO UPDATE
                      SET score = EXCLUDED.score, distance = EXCLUDED.distance, found_at = now()"#,
                &a,
                &b,
                &scores,
                &distances,
            )
            .execute(self.pool())
            .await?
            .rows_affected();
            Ok(usize::try_from(written).unwrap_or(usize::MAX))
        })
        .await
    }

    pub(crate) async fn unjudged_lesson_pairs(
        self,
        project_ids: &[i64],
        limit: i64,
    ) -> Result<Vec<CandidateLessonPair>, PersistError> {
        crate::metrics::time_db("unjudged_lesson_pairs", async move {
            let rows = sqlx::query_as!(
                CandidateLessonPair,
                r#"SELECT a.id AS "lesson_a_id!", a.text AS "lesson_a_text!", a.created_at AS "lesson_a_created_at!",
                          b.id AS "lesson_b_id!", b.text AS "lesson_b_text!", b.created_at AS "lesson_b_created_at!",
                          p.distance AS "distance!"
                     FROM memory.lesson_pair_candidates p
                     JOIN memory.lessons a ON a.id = p.lesson_a_id
                     JOIN memory.lessons b ON b.id = p.lesson_b_id AND b.project_id = a.project_id
                     JOIN memory.lesson_stages sa ON sa.id = a.stage_id AND sa.code IN ('daily', 'long_term')
                     JOIN memory.lesson_stages sb ON sb.id = b.stage_id AND sb.code IN ('daily', 'long_term')
                    WHERE p.judged_at IS NULL
                      AND a.project_id = ANY($1)
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.lesson_relations r
                           WHERE (r.from_lesson_id = a.id AND r.to_lesson_id = b.id)
                              OR (r.from_lesson_id = b.id AND r.to_lesson_id = a.id)
                      )
                    ORDER BY p.score DESC, p.lesson_a_id, p.lesson_b_id
                    LIMIT $2"#,
                project_ids,
                limit,
            )
            .fetch_all(self.pool())
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn mark_lesson_pairs_judged(
        self,
        pairs: &[(i64, i64)],
    ) -> Result<usize, PersistError> {
        if pairs.is_empty() {
            return Ok(0);
        }
        let a: Vec<i64> = pairs.iter().map(|p| p.0).collect();
        let b: Vec<i64> = pairs.iter().map(|p| p.1).collect();
        crate::metrics::time_db("mark_lesson_pairs_judged", async move {
            let marked = sqlx::query!(
                r#"UPDATE memory.lesson_pair_candidates p
                      SET judged_at = now()
                     FROM UNNEST($1::bigint[], $2::bigint[]) AS u(lesson_a_id, lesson_b_id)
                    WHERE p.lesson_a_id = u.lesson_a_id AND p.lesson_b_id = u.lesson_b_id"#,
                &a,
                &b,
            )
            .execute(self.pool())
            .await?
            .rows_affected();
            Ok(usize::try_from(marked).unwrap_or(usize::MAX))
        })
        .await
    }
}
