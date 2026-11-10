use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EarlierTurn {
    pub(crate) session_uuid: Uuid,
    pub(crate) session_title: Option<String>,
    pub(crate) turn_id: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) prompt: String,
    pub(crate) similarity: f64,
}

pub(crate) struct EarlierQuery<'a> {
    pub(crate) project_paths: &'a [String],
    pub(crate) vector_literal: &'a str,
    pub(crate) model: &'a str,
    pub(crate) from: DateTime<Utc>,
    pub(crate) to: DateTime<Utc>,
    pub(crate) exclude_session: Uuid,
    pub(crate) min_similarity: f64,
    pub(crate) limit: i64,
}

impl PersistHandle {
    pub(crate) async fn earlier_sessions(
        &self,
        query: &EarlierQuery<'_>,
    ) -> Result<Vec<EarlierTurn>, PersistError> {
        crate::metrics::time_db("keeper_earlier_sessions", async move {
            let rows = sqlx::query_as!(
                EarlierTurn,
                r#"WITH projects AS (
                       SELECT p.id FROM projects.projects p WHERE p.project_path = ANY($1::text[])
                       UNION
                       SELECT l.project_id FROM projects.locations l WHERE l.root_path = ANY($1::text[])
                   ),
                   scored AS (
                       SELECT s.session_uuid, s.title, t.id AS turn_id, t.created_at,
                              COALESCE(t.user_prompt, '') AS prompt,
                              1 - (c.chunk_embedding <=> $2::text::public.vector) AS similarity
                         FROM memory.operator_turns t
                         JOIN sessions.sessions s ON s.id = t.session_id
                         JOIN memory.operator_turn_chunks c ON c.turn_id = t.id
                        WHERE t.project_id IN (SELECT id FROM projects)
                          AND t.opened_by = 'operator'
                          AND t.created_at BETWEEN $4 AND $5
                          AND c.embedding_model = $3
                          AND s.session_uuid <> $6
                   ),
                   best AS (
                       SELECT DISTINCT ON (session_uuid) *
                         FROM scored
                        ORDER BY session_uuid, similarity DESC, turn_id
                   )
                   SELECT session_uuid AS "session_uuid!",
                          title AS session_title,
                          turn_id AS "turn_id!",
                          created_at AS "created_at!",
                          prompt AS "prompt!",
                          similarity AS "similarity!"
                     FROM best
                    WHERE similarity >= $7
                    ORDER BY similarity DESC, turn_id
                    LIMIT $8"#,
                query.project_paths,
                query.vector_literal,
                query.model,
                query.from,
                query.to,
                query.exclude_session,
                query.min_similarity,
                query.limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
