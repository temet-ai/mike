use uuid::Uuid;

use super::super::PersistError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionKnowledge {
    pub session_id: Uuid,
    pub topics: Vec<String>,
    pub learnings: Vec<String>,
    pub related_session_ids: Vec<Uuid>,
    pub verdict_id: Option<i64>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionLesson {
    pub id: i64,
    pub session_id: Uuid,
    pub kind: String,
    pub stage: String,
    pub text: String,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn upsert_session_knowledge(
        self,
        session_id: Uuid,
        topics: &[String],
        learnings: &[String],
        related_session_ids: &[Uuid],
        verdict_id: Option<i64>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("upsert_session_knowledge", async move {
            sqlx::query!(
                "INSERT INTO memory.session_knowledge
                     (session_id, topics, learnings, related_session_ids, verdict_id, generated_at)
                 VALUES ($1, $2, $3, $4, $5, now())
                 ON CONFLICT (session_id) DO UPDATE
                    SET topics               = EXCLUDED.topics,
                        learnings            = EXCLUDED.learnings,
                        related_session_ids  = EXCLUDED.related_session_ids,
                        verdict_id           = EXCLUDED.verdict_id,
                        generated_at         = EXCLUDED.generated_at",
                session_id,
                topics,
                learnings,
                related_session_ids,
                verdict_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn fetch_session_knowledge(
        self,
        session_id: Uuid,
    ) -> Result<Option<SessionKnowledge>, PersistError> {
        crate::metrics::time_db("fetch_session_knowledge", async move {
            let row = sqlx::query_as!(
                SessionKnowledge,
                r#"SELECT session_id, topics, learnings, related_session_ids,
                          verdict_id, generated_at
                     FROM memory.session_knowledge
                    WHERE session_id = $1"#,
                session_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn fetch_session_knowledge_many(
        self,
        session_ids: &[Uuid],
    ) -> Result<Vec<SessionKnowledge>, PersistError> {
        crate::metrics::time_db("fetch_session_knowledge_many", async move {
            let rows = sqlx::query_as!(
                SessionKnowledge,
                r#"SELECT session_id, topics, learnings, related_session_ids,
                          verdict_id, generated_at
                     FROM memory.session_knowledge
                    WHERE session_id = ANY($1)"#,
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn lessons_from_sessions(
        self,
        session_ids: &[Uuid],
    ) -> Result<Vec<SessionLesson>, PersistError> {
        crate::metrics::time_db("lessons_from_sessions", async move {
            let rows = sqlx::query_as!(
                SessionLesson,
                r#"SELECT l.id,
                          s.session_uuid AS "session_id!",
                          k.code AS "kind!",
                          st.code AS "stage!",
                          l.text
                     FROM memory.lessons l
                     JOIN sessions.sessions s ON s.id = l.session_id
                     JOIN memory.lesson_kinds k ON k.id = l.kind_id
                     JOIN memory.lesson_stages st ON st.id = l.stage_id
                    WHERE s.session_uuid = ANY($1)
                      AND l.archived_at IS NULL
                      AND (l.expires_at IS NULL OR l.expires_at > now())
                    ORDER BY l.created_at DESC"#,
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
