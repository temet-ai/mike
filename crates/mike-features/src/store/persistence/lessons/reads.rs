use super::{LessonCandidateRow, LessonRow, SimilarLesson};
use crate::store::persistence::PersistError;
use crate::store::persistence::operator_turns::vector_to_text_literal;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn list_active_lessons(
        self,
        project_id: i64,
    ) -> Result<Vec<LessonRow>, PersistError> {
        crate::metrics::time_db("list_active_lessons", async move {
            let rows = sqlx::query_as!(
                LessonRow,
                r#"SELECT l.id AS "id!", l.project_id, p.project_path AS "project_path?",
                          lk.code AS "kind!", l.text AS "text!", l.agent_id, ls.code AS "stage!",
                          l.source_turn_id, l.created_at AS "created_at!", l.expires_at, l.topic
                     FROM memory.lessons l
                     LEFT JOIN projects.projects p ON p.id = l.project_id
                     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                    WHERE l.project_id = $1
                      AND ls.code != 'archived'
                      AND (l.expires_at IS NULL OR l.expires_at > NOW())
                    ORDER BY l.created_at DESC"#,
                project_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_lessons_by_topic(
        self,
        project_id: i64,
        topic: &str,
    ) -> Result<Vec<LessonRow>, PersistError> {
        crate::metrics::time_db("list_lessons_by_topic", async move {
            let rows = sqlx::query_as!(
                LessonRow,
                r#"SELECT l.id AS "id!", l.project_id, p.project_path AS "project_path?",
                          lk.code AS "kind!", l.text AS "text!", l.agent_id, ls.code AS "stage!",
                          l.source_turn_id, l.created_at AS "created_at!", l.expires_at, l.topic
                     FROM memory.lessons l
                     LEFT JOIN projects.projects p ON p.id = l.project_id
                     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                    WHERE l.project_id = $1
                      AND l.topic = $2
                      AND l.archived_at IS NULL
                      AND (l.expires_at IS NULL OR l.expires_at > NOW())
                    ORDER BY l.created_at DESC"#,
                project_id,
                topic,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_recall_lessons(
        self,
        project_path: &str,
        topics: &[String],
        per_topic_limit: i64,
    ) -> Result<Vec<LessonRow>, PersistError> {
        let candidates = project_path_candidates(project_path);
        let topics = topics.to_vec();
        crate::metrics::time_db("list_recall_lessons", async move {
            let rows = sqlx::query_as!(
                LessonRow,
                r#"WITH candidates AS (
                       SELECT p.id, p.project_path,
                              array_position($1::text[], p.project_path) AS rank
                         FROM projects.projects p
                        WHERE p.project_path = ANY($1::text[])
                   ),
                   ranked AS (
                       SELECT l.id, l.project_id, c.project_path, lk.code AS kind, l.text,
                              l.agent_id, ls.code AS stage, l.source_turn_id, l.created_at,
                              l.expires_at, l.topic,
                              row_number() OVER (
                                  PARTITION BY l.topic ORDER BY c.rank, l.created_at DESC
                              ) AS position
                         FROM candidates c
                         JOIN memory.lessons l ON l.project_id = c.id
                         JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                         JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                        WHERE l.topic = ANY($2::text[])
                          AND l.archived_at IS NULL
                          AND (l.expires_at IS NULL OR l.expires_at > NOW())
                   )
                   SELECT id AS "id!", project_id AS "project_id?", project_path AS "project_path?",
                          kind AS "kind!", text AS "text!", agent_id AS "agent_id?",
                          stage AS "stage!", source_turn_id AS "source_turn_id?",
                          created_at AS "created_at!", expires_at AS "expires_at?",
                          topic AS "topic?"
                     FROM ranked
                    WHERE position <= $3
                    ORDER BY topic, position"#,
                &candidates,
                &topics,
                per_topic_limit
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn get_project_ids_for_path(
        self,
        project_path: &str,
    ) -> Result<Vec<i64>, PersistError> {
        let candidates = project_path_candidates(project_path);
        crate::metrics::time_db("get_project_ids_for_path", async move {
            let ids = sqlx::query_scalar!(
                r#"SELECT id AS "id!" FROM projects.projects
                    WHERE project_path = ANY($1::text[])
                    ORDER BY array_position($1::text[], project_path)"#,
                &candidates,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }

    pub async fn find_similar_lessons(
        self,
        project_id: i64,
        query_embedding: &[f32],
        agent_id: Option<i64>,
        limit: i64,
        exclude_id: i64,
    ) -> Result<Vec<SimilarLesson>, PersistError> {
        crate::metrics::time_db("find_similar_lessons", async move {
            let vec_literal = vector_to_text_literal(query_embedding);
            let rows = match agent_id {
                Some(agent_id) => {
                    sqlx::query_as!(
                        SimilarLesson,
                        r#"SELECT l.id AS "id!", l.uuid AS "uuid!", l.text AS "text!",
                                  (l.embedding <=> $2::text::vector)::float4 AS "distance!"
                             FROM memory.lessons l
                             JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                            WHERE l.project_id = $1
                              AND l.embedding IS NOT NULL
                              AND l.id <> $3
                              AND (l.agent_id IS NULL OR l.agent_id = $4)
                            ORDER BY l.embedding <=> $2::text::vector
                            LIMIT $5"#,
                        project_id,
                        vec_literal,
                        exclude_id,
                        agent_id,
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_as!(
                        SimilarLesson,
                        r#"SELECT l.id AS "id!", l.uuid AS "uuid!", l.text AS "text!",
                                  (l.embedding <=> $2::text::vector)::float4 AS "distance!"
                             FROM memory.lessons l
                             JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code IN ('daily', 'long_term')
                            WHERE l.project_id = $1
                              AND l.embedding IS NOT NULL
                              AND l.id <> $3
                            ORDER BY l.embedding <=> $2::text::vector
                            LIMIT $4"#,
                        project_id,
                        vec_literal,
                        exclude_id,
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            Ok(rows)
        })
        .await
    }

    pub async fn list_lesson_candidates(
        self,
        limit: i64,
    ) -> Result<Vec<LessonCandidateRow>, PersistError> {
        crate::metrics::time_db("list_lesson_candidates", async move {
            let rows = sqlx::query_file_as!(
                LessonCandidateRow,
                "src/store/persistence/lessons/lesson_candidate_list.sql",
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn get_lesson_candidate(
        self,
        lesson_id: i64,
    ) -> Result<Option<LessonCandidateRow>, PersistError> {
        crate::metrics::time_db("get_lesson_candidate", async move {
            let row = sqlx::query_file_as!(
                LessonCandidateRow,
                "src/store/persistence/lessons/lesson_candidate_get.sql",
                lesson_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}

fn project_path_candidates(project_path: &str) -> Vec<String> {
    let mut candidates = vec![project_path.to_owned()];
    if let Some(alias) = crate::extract::request::legacy_trimmed_alias(project_path) {
        candidates.push(alias);
    }
    candidates
}
