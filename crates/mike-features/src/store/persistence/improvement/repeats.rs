use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct PushbackPrompt {
    pub turn_id: i64,
    pub session_id: Uuid,
    pub at: DateTime<Utc>,
    pub project_path: String,
    pub text: String,
    pub embedding: Vec<f32>,
}

const MIN_CHARS: i32 = 20;
const MAX_CHARS: i32 = 1500;

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn list_pushback_prompts_with_vectors(
        self,
        since: DateTime<Utc>,
    ) -> Result<Vec<PushbackPrompt>, PersistError> {
        crate::metrics::time_db("list_pushback_prompts_with_vectors", async move {
            let rows = sqlx::query!(
                r#"SELECT t.id AS turn_id,
                          s.session_uuid,
                          t.created_at,
                          COALESCE(p.project_path, '') AS "project_path!",
                          t.user_prompt AS "text!",
                          c.chunk_embedding::real[] AS "embedding!"
                     FROM memory.operator_turns t
                     JOIN memory.turn_facts f ON f.operator_turn_id = t.id
                     JOIN sessions.sessions s ON s.id = t.session_id
                     LEFT JOIN projects.projects p ON p.id = t.project_id
                     JOIN memory.operator_turn_chunks c ON c.turn_id = t.id AND c.chunk_idx = 0
                     JOIN memory.chunk_roles r ON r.id = c.chunk_role_id AND r.code = 'prompt'
                    WHERE t.created_at >= $1
                      AND f.prompt_kind = 'genuine'
                      AND t.reaction IN ('corrected', 'redirected')
                      AND (t.subject_agent IS NULL OR t.subject_agent = 'main')
                      AND t.user_prompt IS NOT NULL
                      AND length(t.user_prompt) BETWEEN $2 AND $3
                      AND t.user_prompt !~ '^[A-Za-z0-9_-]+\. Your (brief|handover) is saved at'
                      AND t.user_prompt !~ '^<session>'
                      AND t.user_prompt NOT LIKE '%[Image: source:%'
                      AND t.user_prompt NOT LIKE '%[Request interrupted by user]%'"#,
                since,
                MIN_CHARS,
                MAX_CHARS
            )
            .fetch_all(&self.pool)
            .await?;
            let mut prompts: Vec<PushbackPrompt> = rows
                .into_iter()
                .map(|r| PushbackPrompt {
                    turn_id: r.turn_id,
                    session_id: r.session_uuid,
                    at: r.created_at,
                    project_path: r.project_path,
                    text: r.text,
                    embedding: r.embedding,
                })
                .collect();
            prompts.sort_by_key(|p| (p.at, p.turn_id));
            Ok(prompts)
        })
        .await
    }
}
