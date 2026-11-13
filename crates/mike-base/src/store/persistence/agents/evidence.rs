use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};
use crate::agents::{Agent, AgentSource};

struct AgentWithEvidenceRow {
    id: i64,
    name: String,
    definition_path: Option<String>,
    description: Option<String>,
    source: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    composition_count: i64,
    dispatch_count: i64,
    instance_count: i64,
    prompt_version_count: i64,
    last_dispatched_at: Option<DateTime<Utc>>,
}

impl TryFrom<AgentWithEvidenceRow> for AgentWithEvidence {
    type Error = PersistError;
    fn try_from(row: AgentWithEvidenceRow) -> Result<Self, Self::Error> {
        let source = AgentSource::parse_str(&row.source).map_err(|err| {
            PersistError::DataInvariant(format!(
                "agents row id={} carries invalid source: {err}",
                row.id
            ))
        })?;
        let agent = Agent {
            id: row.id,
            name: row.name,
            definition_path: row.definition_path,
            description: row.description,
            source,
            created_at: row.created_at,
            updated_at: row.updated_at,
        };
        Ok(AgentWithEvidence {
            agent,
            composition_count: row.composition_count,
            dispatch_count: row.dispatch_count,
            instance_count: row.instance_count,
            prompt_version_count: row.prompt_version_count,
            last_dispatched_at: row.last_dispatched_at,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgentWithEvidence {
    pub agent: Agent,
    pub composition_count: i64,
    pub dispatch_count: i64,
    pub instance_count: i64,
    pub prompt_version_count: i64,
    pub last_dispatched_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentDispatchRow {
    pub tool_use_id: i64,
    pub dispatched_at: DateTime<Utc>,
    pub prompt: String,
    pub session_uuid: uuid::Uuid,
    pub request_id: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentPromptVersionRow {
    pub content_sha256: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub request_count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentInstanceRow {
    pub id: i64,
    pub session_uuid: uuid::Uuid,
    pub subagent_persona: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub first_request_id: Option<i64>,
}

impl PersistHandle {
    pub async fn list_agents_with_evidence(&self) -> Result<Vec<AgentWithEvidence>, PersistError> {
        crate::metrics::time_db("list_agents_with_evidence", async move {
            let rows = sqlx::query_file_as!(
                AgentWithEvidenceRow,
                "src/store/persistence/agents/evidence_all.sql",
            )
            .fetch_all(&self.pool)
            .await?;

            rows.into_iter().map(AgentWithEvidence::try_from).collect()
        })
        .await
    }

    pub async fn fetch_agent_evidence_by_id(
        &self,
        agent_id: i64,
    ) -> Result<Option<AgentWithEvidence>, PersistError> {
        crate::metrics::time_db("fetch_agent_evidence_by_id", async move {
            let row = sqlx::query_file_as!(
                AgentWithEvidenceRow,
                "src/store/persistence/agents/evidence_one.sql",
                agent_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(r) => Ok(Some(AgentWithEvidence::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn count_dispatches_for_agent(&self, agent_id: i64) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_dispatches_for_agent", async move {
            let count = sqlx::query_scalar!(
                "SELECT COUNT(*) AS \"count!\" FROM agents.dispatches WHERE agent_id = $1",
                agent_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(count)
        })
        .await
    }

    pub async fn list_dispatches_for_agent(
        &self,
        agent_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AgentDispatchRow>, PersistError> {
        crate::metrics::time_db("list_dispatches_for_agent", async move {
            let rows = sqlx::query_as!(
                AgentDispatchRow,
                r#"SELECT ad.tool_use_id AS "tool_use_id!",
                          tu.created_at AS "dispatched_at!",
                          COALESCE(tu.input->>'prompt', '') AS "prompt!",
                          s.session_uuid AS "session_uuid!",
                          r.id AS "request_id!"
                     FROM agents.dispatches ad
                     JOIN tools.uses tu ON tu.id = ad.tool_use_id
                     JOIN requests.requests r ON r.id = tu.request_id
                     JOIN sessions.sessions s ON s.id = r.session_id
                    WHERE ad.agent_id = $1
                    ORDER BY tu.created_at DESC, ad.id DESC
                    LIMIT $2 OFFSET $3"#,
                agent_id,
                limit,
                offset,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_prompt_versions_for_agent(
        &self,
        agent_id: i64,
    ) -> Result<Vec<AgentPromptVersionRow>, PersistError> {
        crate::metrics::time_db("list_prompt_versions_for_agent", async move {
            let rows = sqlx::query_as!(
                AgentPromptVersionRow,
                r#"SELECT content_sha256 AS "content_sha256!",
                          MIN(created_at) AS "first_seen_at!",
                          MAX(created_at) AS "last_seen_at!",
                          COUNT(*) AS "request_count!"
                     FROM system_prompts.session_prompts
                    WHERE agent_id = $1
                      AND content_sha256 IS NOT NULL
                    GROUP BY content_sha256
                    ORDER BY MAX(created_at) DESC"#,
                agent_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_instances_for_agent(
        &self,
        agent_id: i64,
    ) -> Result<Vec<AgentInstanceRow>, PersistError> {
        crate::metrics::time_db("list_instances_for_agent", async move {
            let rows = sqlx::query_as!(
                AgentInstanceRow,
                r#"SELECT ai.id, s.session_uuid AS "session_uuid!",
                          ai.subagent_persona, ai.started_at, ai.first_request_id
                     FROM agents.instances ai
                     JOIN sessions.sessions s ON s.id = ai.session_id
                    WHERE ai.agent_id = $1
                    ORDER BY ai.id DESC"#,
                agent_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
