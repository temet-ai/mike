
use chrono::{DateTime, Utc};

use super::{PersistError, PersistHandle};
use crate::agents::{Agent, AgentSource};

#[derive(sqlx::FromRow)]
struct AgentRow {
    id: i64,
    name: String,
    definition_path: Option<String>,
    description: Option<String>,
    source: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<AgentRow> for Agent {
    type Error = PersistError;
    fn try_from(row: AgentRow) -> Result<Self, Self::Error> {
        let source = AgentSource::from_str(&row.source).map_err(|err| {
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

#[derive(sqlx::FromRow)]
struct AgentWithCountRow {
    id: i64,
    name: String,
    definition_path: Option<String>,
    description: Option<String>,
    source: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version_count: i64,
}

impl TryFrom<AgentWithCountRow> for AgentWithVersionCount {
    type Error = PersistError;
    fn try_from(row: AgentWithCountRow) -> Result<Self, Self::Error> {
        let source = AgentSource::from_str(&row.source).map_err(|err| {
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
        Ok(AgentWithVersionCount {
            agent,
            version_count: row.version_count,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentWithVersionCount {
    pub(crate) agent: Agent,
    pub(crate) version_count: i64,
}

#[derive(Debug, Default)]
pub(crate) struct PatchAgentParams<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) definition_path: Option<Option<&'a str>>,
    pub(crate) description: Option<Option<&'a str>>,
    pub(crate) source: Option<AgentSource>,
}

impl PersistHandle {
    pub(crate) async fn list_agents_with_counts(
        &self,
    ) -> Result<Vec<AgentWithVersionCount>, PersistError> {
        crate::metrics::time_db("list_agents_with_counts", async move {
            let rows = sqlx::query_as!(
                AgentWithCountRow,
                r#"SELECT a.id, a.name, a.definition_path, a.description,
                        s.code AS "source!",
                        a.created_at, a.updated_at,
                        COALESCE(c.version_count, 0)::bigint AS "version_count!"
                   FROM agents.agents a
                   JOIN agents.sources s ON s.id = a.source_id
                   LEFT JOIN (
                       SELECT agent_id, COUNT(*) AS version_count
                         FROM overrides.overrides
                        WHERE agent_id IS NOT NULL
                        GROUP BY agent_id
                   ) c ON c.agent_id = a.id
                  ORDER BY a.updated_at DESC"#,
            )
            .fetch_all(&self.pool)
            .await?;

            rows.into_iter()
                .map(AgentWithVersionCount::try_from)
                .collect()
        })
        .await
    }

    pub(crate) async fn fetch_agent_by_id(&self, id: i64) -> Result<Option<Agent>, PersistError> {
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

    pub(crate) async fn fetch_agent_by_name(
        &self,
        name: &str,
    ) -> Result<Option<Agent>, PersistError> {
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

    pub(crate) async fn patch_agent(
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

    pub(crate) async fn delete_agent(&self, id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("delete_agent", async move {
            let result = sqlx::query!("DELETE FROM agents.agents WHERE id = $1", id)
                .execute(&self.pool)
                .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub(crate) async fn list_compositions_for_agent(
        &self,
        agent_id: i64,
    ) -> Result<Vec<crate::compose_prompt::decision::Composition>, PersistError> {
        crate::metrics::time_db("list_compositions_for_agent", async move {
            let rows = sqlx::query_as!(
                crate::store::persistence::compositions::CompositionRow,
                r#"SELECT pc.id, pc.uuid, pc.prompt_sha256, pc.label, pc.blocks,
                          m.code AS "mode!",
                          pc.notes, pc.agent_id AS "agent_id?", pc.agent_name,
                          pc.agent_definition_path, pc.version, pc.created_at, pc.updated_at
                   FROM overrides.overrides pc
                   JOIN overrides.modes m ON m.id = pc.mode_id
                  WHERE pc.agent_id = $1
                  ORDER BY pc.created_at DESC"#,
                agent_id,
            )
            .fetch_all(&self.pool)
            .await?;
            rows.into_iter()
                .map(crate::compose_prompt::decision::Composition::try_from)
                .collect()
        })
        .await
    }

    pub(crate) async fn bulk_upsert_file_backed_agents(
        &self,
        names: &[String],
        paths: &[String],
        descriptions: &[Option<String>],
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("bulk_upsert_file_backed_agents", async move {
            let affected: i64 = sqlx::query_scalar!(
                "WITH incoming AS (
                     SELECT n, p, d
                       FROM UNNEST($1::TEXT[], $2::TEXT[], $3::TEXT[]) AS t(n, p, d)
                 ),
                 file_backed AS (
                     SELECT id FROM agents.sources WHERE code = 'file_backed' LIMIT 1
                 ),
                 observed_source AS (
                     SELECT id FROM agents.sources WHERE code = 'observed' LIMIT 1
                 ),
                 upd AS (
                     UPDATE agents.agents a
                        SET source_id = CASE
                                WHEN a.source_id = obs.id THEN fb.id
                                ELSE a.source_id
                            END,
                            definition_path = COALESCE(a.definition_path, i.p),
                            description     = CASE
                                WHEN a.description IS NULL
                                    THEN i.d
                                WHEN a.description ~ '^[>|][-+]?$'
                                    THEN i.d
                                ELSE a.description
                            END,
                            updated_at      = NOW()
                       FROM incoming i, file_backed fb, observed_source obs
                      WHERE a.name = i.n
                     RETURNING a.name
                 ),
                 ins AS (
                     INSERT INTO agents.agents (name, definition_path, description, source_id)
                     SELECT i.n, i.p, i.d, fb.id
                       FROM incoming i, file_backed fb
                      WHERE NOT EXISTS (
                          SELECT 1 FROM agents.agents a WHERE a.name = i.n
                      )
                     RETURNING name
                 )
                 SELECT ((SELECT COUNT(*) FROM upd) + (SELECT COUNT(*) FROM ins))::BIGINT",
                names as &[String],
                paths as &[String],
                descriptions as &[Option<String>],
            )
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);
            Ok(affected)
        })
        .await
    }
}

pub(crate) async fn find_or_create_agent_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    name: &str,
    definition_path: Option<&str>,
) -> Result<(Agent, i64), PersistError> {
    let row = sqlx::query_as!(
        AgentRow,
        r#"WITH existing AS (
                SELECT a.id, a.name, a.definition_path, a.description,
                       s.code AS source_code, a.created_at, a.updated_at
                  FROM agents.agents a
                  JOIN agents.sources s ON s.id = a.source_id
                 WHERE a.name = $1
                 ORDER BY a.updated_at DESC LIMIT 1
            ),
            ins AS (
                INSERT INTO agents.agents (name, definition_path)
                SELECT $1, $2
                 WHERE NOT EXISTS (SELECT 1 FROM existing)
                RETURNING id, name, definition_path, description, source_id,
                          created_at, updated_at
            ),
            ins_full AS (
                SELECT i.id, i.name, i.definition_path, i.description,
                       s.code AS source_code, i.created_at, i.updated_at
                  FROM ins i
                  JOIN agents.sources s ON s.id = i.source_id
            )
            SELECT id AS "id!", name AS "name!", definition_path, description,
                   source_code AS "source!", created_at AS "created_at!",
                   updated_at AS "updated_at!"
              FROM (
                  SELECT id, name, definition_path, description,
                         source_code, created_at, updated_at FROM ins_full
                  UNION ALL
                  SELECT id, name, definition_path, description,
                         source_code, created_at, updated_at FROM existing
                  LIMIT 1
              ) picked"#,
        name,
        definition_path,
    )
    .fetch_one(&mut **tx)
    .await?;
    let db_id = row.id;
    let agent = Agent::try_from(row)?;
    Ok((agent, db_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose_prompt::decision::CompositionMode;

    #[tokio::test]
    async fn list_agents_with_counts_counts_linked_compositions() -> anyhow::Result<()> {
        use crate::store::persistence::test_support::acquire_ephemeral_db;

        let Some(db) = acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let blocks = serde_json::json!([
            {"text": "billing-header"},
            {"text": "identity preamble"},
            {"text": "counts-test-body"},
        ]);

        let comp_a = handle
            .create_composition(
                &"a".repeat(64),
                "linked-a",
                &blocks,
                Some(CompositionMode::Observe),
                Some("counts-agent"),
                Some("/tmp/counts-agent.md"),
            )
            .await?;
        handle
            .create_composition(
                &"b".repeat(64),
                "linked-b",
                &blocks,
                Some(CompositionMode::Observe),
                Some("counts-agent"),
                None,
            )
            .await?;
        let linked_agent_id = comp_a
            .agent_id
            .ok_or_else(|| anyhow::anyhow!("linked composition agent_id missing"))?;

        handle
            .create_composition(
                &"c".repeat(64),
                "unlinked-comp",
                &blocks,
                Some(CompositionMode::Observe),
                None,
                None,
            )
            .await?;
        let lonely_agent_id: i64 = sqlx::query_scalar!(
            "INSERT INTO agents.agents (name) VALUES ($1) RETURNING id",
            "lonely-agent",
        )
        .fetch_one(&handle.pool)
        .await?;

        let agents = handle.list_agents_with_counts().await?;
        let linked = agents
            .iter()
            .find(|a| a.agent.id == linked_agent_id)
            .ok_or_else(|| anyhow::anyhow!("linked agent missing from list"))?;
        assert_eq!(
            linked.version_count, 2,
            "agent linked to two compositions must report version_count = 2"
        );
        let lonely = agents
            .iter()
            .find(|a| a.agent.id == lonely_agent_id)
            .ok_or_else(|| anyhow::anyhow!("lonely agent missing from list"))?;
        assert_eq!(
            lonely.version_count, 0,
            "agent with no compositions must COALESCE to version_count = 0"
        );

        let comps = handle.list_compositions_for_agent(linked_agent_id).await?;
        assert_eq!(
            comps.len(),
            2,
            "list_compositions_for_agent must return both linked compositions"
        );
        for comp in &comps {
            assert_eq!(
                comp.agent_id,
                Some(linked_agent_id),
                "each returned composition must carry the agent bigint id via the FK"
            );
        }

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn bulk_upsert_file_backed_agents_inserts_and_promotes() -> anyhow::Result<()> {
        use crate::store::persistence::test_support::acquire_ephemeral_db;

        let Some(db) = acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        sqlx::query!(
            "INSERT INTO agents.agents (name, source_id)
             SELECT $1, s.id FROM agents.sources s WHERE s.code = 'observed'",
            "promote-me",
        )
        .execute(&handle.pool)
        .await?;

        let names = vec!["bulk-a".to_string(), "bulk-b".to_string()];
        let paths = vec!["/tmp/bulk-a.md".to_string(), "/tmp/bulk-b.md".to_string()];
        let descriptions = vec![Some("desc-a".to_string()), None];
        let inserted = handle
            .bulk_upsert_file_backed_agents(&names, &paths, &descriptions)
            .await?;
        assert_eq!(inserted, 2, "two new rows must be reported as upserted");

        #[derive(sqlx::FromRow)]
        struct AgentShape {
            source: String,
            definition_path: Option<String>,
        }
        let a = sqlx::query_as!(
            AgentShape,
            r#"SELECT s.code AS "source!", a.definition_path
                 FROM agents.agents a
                 JOIN agents.sources s ON s.id = a.source_id
                WHERE a.name = $1"#,
            "bulk-a",
        )
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(a.source, "file_backed");
        assert_eq!(a.definition_path.as_deref(), Some("/tmp/bulk-a.md"));

        let promote_names = vec!["promote-me".to_string()];
        let promote_paths = vec!["/tmp/promote-me.md".to_string()];
        let promote_descs = vec![Some("promoted desc".to_string())];
        let promoted = handle
            .bulk_upsert_file_backed_agents(&promote_names, &promote_paths, &promote_descs)
            .await?;
        assert_eq!(
            promoted, 1,
            "the promote upsert must report one affected row"
        );

        let p = sqlx::query_as!(
            AgentShape,
            r#"SELECT s.code AS "source!", a.definition_path
                 FROM agents.agents a
                 JOIN agents.sources s ON s.id = a.source_id
                WHERE a.name = $1"#,
            "promote-me",
        )
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            p.source, "file_backed",
            "observed source must be promoted to file_backed on conflict"
        );
        assert_eq!(
            p.definition_path.as_deref(),
            Some("/tmp/promote-me.md"),
            "blank definition_path must be filled via COALESCE on conflict"
        );

        db.cleanup().await?;
        Ok(())
    }
}
