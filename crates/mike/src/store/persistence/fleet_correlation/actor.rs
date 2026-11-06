use std::collections::HashSet;

use uuid::Uuid;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Default)]
pub(crate) struct ActorEnrichment {
    pub(crate) declared_role: Option<String>,
    pub(crate) agent_type: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) next_steps: Option<String>,
    pub(crate) files: Vec<String>,
}

impl PersistHandle {
    pub(crate) async fn fetch_actor_enrichment(
        &self,
        session_id: i64,
    ) -> Result<ActorEnrichment, PersistError> {
        crate::metrics::time_db("fleet_fetch_actor_enrichment", async move {
            let agent_type = sqlx::query_scalar!(
                r#"SELECT ag.name
                     FROM agents.instances ai
                     JOIN agents.agents ag ON ag.id = ai.agent_id
                    WHERE ai.session_id = $1
                    ORDER BY ai.created_at DESC
                    LIMIT 1"#,
                session_id,
            )
            .fetch_optional(&self.pool)
            .await?;

            let goal_row = sqlx::query!(
                r#"SELECT goal AS "goal!", next_steps AS "next_steps?", role AS "role?"
                     FROM sessions.session_goals
                    WHERE session_id = $1"#,
                session_id,
            )
            .fetch_optional(&self.pool)
            .await?;

            let files = sqlx::query_scalar!(
                r#"SELECT DISTINCT tu.file_path AS "file_path!"
                     FROM tools.uses tu
                    WHERE tu.session_id = $1 AND tu.file_path IS NOT NULL
                    ORDER BY tu.file_path"#,
                session_id,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(ActorEnrichment {
                declared_role: goal_row.as_ref().and_then(|r| r.role.clone()),
                agent_type,
                goal: goal_row.as_ref().map(|r| r.goal.clone()),
                next_steps: goal_row.and_then(|r| r.next_steps),
                files,
            })
        })
        .await
    }

    pub(crate) async fn fetch_captured_session_uuids(
        &self,
        uuids: &[Uuid],
    ) -> Result<HashSet<Uuid>, PersistError> {
        crate::metrics::time_db("fleet_fetch_captured_session_uuids", async move {
            let rows = sqlx::query_scalar!(
                r#"SELECT session_uuid AS "session_uuid!" FROM sessions.sessions WHERE session_uuid = ANY($1)"#,
                uuids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().collect())
        })
        .await
    }
}
