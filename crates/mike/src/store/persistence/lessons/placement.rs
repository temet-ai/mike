use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementWrite<'a> {
    pub home: &'a str,
    pub covered_by: Option<&'a str>,
    pub reason: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnplacedLesson {
    pub id: i64,
    pub project_id: i64,
    pub text: String,
    pub kind: String,
    pub topic: Option<String>,
    pub agent_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPurpose {
    pub project_id: i64,
    pub topic: String,
    pub lessons: i64,
    pub shipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicMove {
    pub from: String,
    pub to: String,
}

impl PersistHandle {
    pub async fn unplaced_lessons(&self, limit: i64) -> Result<Vec<UnplacedLesson>, PersistError> {
        crate::metrics::time_db("unplaced_lessons", async move {
            Ok(sqlx::query_as!(
                UnplacedLesson,
                r#"SELECT l.id, l.project_id AS "project_id!", l.text, k.code AS "kind!",
                          l.topic, a.name AS "agent_name?"
                     FROM memory.lessons l
                     JOIN memory.lesson_kinds k ON k.id = l.kind_id
                     LEFT JOIN agents.agents a ON a.id = l.agent_id
                     LEFT JOIN memory.lesson_placement_failures f ON f.lesson_id = l.id
                    WHERE l.placement_id IS NULL
                      AND l.archived_at IS NULL
                      AND l.project_id IS NOT NULL
                      AND COALESCE(l.topic, '') NOT LIKE 'verifier:%'
                      AND (f.lesson_id IS NULL
                           OR (f.attempts < 3
                               AND f.last_failed_at <= now() - interval '1 hour' * (2 ^ f.attempts)))
                    ORDER BY l.id
                    LIMIT $1"#,
                limit
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn record_placement_failure(
        &self,
        lesson_id: i64,
        error: &str,
    ) -> Result<i32, PersistError> {
        crate::metrics::time_db("record_placement_failure", async move {
            let attempts = sqlx::query_scalar!(
                "INSERT INTO memory.lesson_placement_failures (lesson_id, last_error)
                 VALUES ($1, $2)
                 ON CONFLICT (lesson_id) DO UPDATE
                    SET attempts = memory.lesson_placement_failures.attempts + 1,
                        last_error = EXCLUDED.last_error,
                        last_failed_at = now()
                 RETURNING attempts",
                lesson_id,
                error
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(attempts)
        })
        .await
    }

    pub async fn set_lesson_placement(
        &self,
        lesson_id: i64,
        placement: PlacementWrite<'_>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_lesson_placement", async move {
            let result = sqlx::query!(
                "UPDATE memory.lessons l
                    SET placement_id = lp.id,
                        placement_covered_by = $3,
                        placement_reason = $4,
                        placed_at = now()
                   FROM memory.lesson_placements lp
                  WHERE lp.code = $2
                    AND l.id = $1
                    AND l.placement_id IS NULL",
                lesson_id,
                placement.home,
                placement.covered_by,
                placement.reason
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub async fn skill_purposes(&self) -> Result<Vec<SkillPurpose>, PersistError> {
        crate::metrics::time_db("skill_purposes", async move {
            Ok(sqlx::query_as!(
                SkillPurpose,
                r#"WITH skill_topics AS (
                       SELECT l.project_id, l.topic, count(*) AS lessons
                         FROM memory.lessons l
                         JOIN memory.lesson_stages ls
                           ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                         JOIN memory.lesson_placements lp
                           ON lp.id = l.placement_id AND lp.code = 'skill'
                        WHERE l.archived_at IS NULL
                          AND l.topic IS NOT NULL
                          AND l.topic NOT LIKE 'shared:%'
                          AND l.topic NOT LIKE 'verifier:%'
                          AND l.project_id IS NOT NULL
                        GROUP BY l.project_id, l.topic),
                   shipped AS (
                       SELECT DISTINCT d.project_id, d.topic
                         FROM memory.skill_drafts d
                        WHERE d.status = 'accepted' AND d.topic IS NOT NULL)
                   SELECT t.project_id AS "project_id!", t.topic AS "topic!",
                          t.lessons AS "lessons!", (s.topic IS NOT NULL) AS "shipped!"
                     FROM skill_topics t
                     LEFT JOIN shipped s ON s.project_id = t.project_id AND s.topic = t.topic
                    ORDER BY t.project_id, t.topic"#
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn retopic_skill_lessons(
        &self,
        project_id: i64,
        moves: &[TopicMove],
    ) -> Result<u64, PersistError> {
        if moves.is_empty() {
            return Ok(0);
        }
        let from: Vec<String> = moves.iter().map(|m| m.from.clone()).collect();
        let to: Vec<String> = moves.iter().map(|m| m.to.clone()).collect();
        crate::metrics::time_db("retopic_skill_lessons", async move {
            let result = sqlx::query!(
                "UPDATE memory.lessons l
                    SET topic = m.to_topic
                   FROM unnest($2::text[], $3::text[]) AS m(from_topic, to_topic),
                        memory.lesson_placements lp
                  WHERE lp.id = l.placement_id AND lp.code = 'skill'
                    AND l.project_id = $1
                    AND l.topic = m.from_topic
                    AND l.archived_at IS NULL",
                project_id,
                &from,
                &to
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }
}
