use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};
use crate::agents::{Agent, AgentSource};

#[derive(sqlx::FromRow)]
pub struct AgentRow {
    pub id: i64,
    pub name: String,
    pub definition_path: Option<String>,
    pub description: Option<String>,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<AgentRow> for Agent {
    type Error = PersistError;
    fn try_from(row: AgentRow) -> Result<Self, Self::Error> {
        let source = AgentSource::parse_str(&row.source).map_err(|err| {
            PersistError::DataInvariant(format!(
                "agents row id={} carries invalid source: {err}",
                row.id
            ))
        })?;
        Ok(Agent {
            id: row.id,
            name: row.name,
            definition_path: row.definition_path,
            description: row.description,
            source,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, Default)]
pub struct PatchAgentParams<'a> {
    pub name: Option<&'a str>,
    pub definition_path: Option<Option<&'a str>>,
    pub description: Option<Option<&'a str>>,
    pub source: Option<AgentSource>,
}

impl PersistHandle {
    pub async fn fetch_agent_by_id(&self, id: i64) -> Result<Option<Agent>, PersistError> {
        crate::metrics::time_db("fetch_agent_by_id", async move {
            let row = sqlx::query_as!(
                AgentRow,
                r#"SELECT a.id, a.name, a.definition_path, a.description,
                          s.code AS "source!", a.created_at, a.updated_at
                     FROM agents.agents a
                     JOIN agents.sources s ON s.id = a.source_id
                    WHERE a.id = $1"#,
                id,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(r) => Ok(Some(Agent::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn fetch_agent_by_name(&self, name: &str) -> Result<Option<Agent>, PersistError> {
        crate::metrics::time_db("fetch_agent_by_name", async move {
            let row = sqlx::query_as!(
                AgentRow,
                r#"SELECT a.id, a.name, a.definition_path, a.description,
                          s.code AS "source!", a.created_at, a.updated_at
                     FROM agents.agents a
                     JOIN agents.sources s ON s.id = a.source_id
                    WHERE a.name = $1
                    ORDER BY a.updated_at DESC
                    LIMIT 1"#,
                name,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(r) => Ok(Some(Agent::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn patch_agent(
        &self,
        id: i64,
        params: PatchAgentParams<'_>,
    ) -> Result<Option<Agent>, PersistError> {
        crate::metrics::time_db("patch_agent", async move {
            let definition_path_set = params.definition_path.is_some();
            let definition_path_value: Option<&str> = params.definition_path.flatten();
            let description_set = params.description.is_some();
            let description_value: Option<&str> = params.description.flatten();
            let source_str = params.source.map(|s| s.as_str());

            let row = sqlx::query_as!(
                AgentRow,
                r#"WITH updated AS (
                    UPDATE agents.agents
                       SET name            = COALESCE($2, name),
                           definition_path = CASE WHEN $3 THEN $4 ELSE definition_path END,
                           description     = CASE WHEN $5 THEN $6 ELSE description END,
                           source_id       = COALESCE(
                               (SELECT id FROM agents.sources WHERE code = $7 LIMIT 1),
                               source_id
                           ),
                           updated_at      = NOW()
                     WHERE id = $1
                   RETURNING id, name, definition_path, description, source_id,
                             created_at, updated_at
                )
                SELECT u.id, u.name, u.definition_path, u.description,
                       s.code AS "source!", u.created_at, u.updated_at
                  FROM updated u
                  JOIN agents.sources s ON s.id = u.source_id"#,
                id,
                params.name,
                definition_path_set,
                definition_path_value,
                description_set,
                description_value,
                source_str,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(r) => Ok(Some(Agent::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub async fn delete_agent(&self, id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("delete_agent", async move {
            let result = sqlx::query!("DELETE FROM agents.agents WHERE id = $1", id)
                .execute(&self.pool)
                .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }
}
