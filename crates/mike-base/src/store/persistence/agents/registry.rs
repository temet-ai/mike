use super::super::{PersistError, PersistHandle};
use crate::agents::Agent;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentAlias {
    pub alias: String,
    pub agent_id: i64,
    pub kind: String,
}

impl PersistHandle {
    pub async fn bulk_upsert_file_backed_agents(
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
                 upserted AS (
                     INSERT INTO agents.agents (name, definition_path, description, source_id)
                     SELECT i.n, i.p, i.d, fb.id
                       FROM incoming i, file_backed fb
                     ON CONFLICT (LOWER(name)) DO UPDATE
                        SET source_id       = (SELECT fb2.id FROM agents.sources fb2
                                               WHERE fb2.code = 'file_backed' LIMIT 1),
                            definition_path = COALESCE(agents.agents.definition_path, EXCLUDED.definition_path),
                            description     = CASE
                                WHEN agents.agents.description IS NULL
                                    THEN EXCLUDED.description
                                WHEN agents.agents.description ~ '^[>|][-+]?$'
                                    THEN EXCLUDED.description
                                ELSE agents.agents.description
                            END,
                            updated_at      = NOW()
                      WHERE agents.agents.source_id IS DISTINCT FROM (
                                SELECT fb3.id FROM agents.sources fb3
                                 WHERE fb3.code = 'file_backed' LIMIT 1)
                         OR agents.agents.definition_path IS DISTINCT FROM
                                COALESCE(agents.agents.definition_path, EXCLUDED.definition_path)
                         OR agents.agents.description IS DISTINCT FROM CASE
                                WHEN agents.agents.description IS NULL
                                    THEN EXCLUDED.description
                                WHEN agents.agents.description ~ '^[>|][-+]?$'
                                    THEN EXCLUDED.description
                                ELSE agents.agents.description
                            END
                     RETURNING name
                 )
                 SELECT COUNT(*)::BIGINT FROM upserted",
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

    pub async fn fetch_agent_ids_by_names(
        &self,
        names: &[String],
    ) -> Result<Vec<(String, i64)>, PersistError> {
        crate::metrics::time_db("fetch_agent_ids_by_names", async move {
            let rows = sqlx::query!(
                "SELECT name, id FROM agents.agents WHERE name = ANY($1::TEXT[])",
                names as &[String],
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| (r.name, r.id)).collect())
        })
        .await
    }

    pub async fn fetch_alias_map(&self) -> Result<Vec<AgentAlias>, PersistError> {
        let mut aliases = self.fetch_resolution_map().await?;
        aliases.retain(|a| a.kind != "name");
        Ok(aliases)
    }

    pub async fn fetch_resolution_map(&self) -> Result<Vec<AgentAlias>, PersistError> {
        crate::metrics::time_db("fetch_alias_map", async move {
            let rows = sqlx::query_as!(
                AgentAlias,
                r#"SELECT alternate.alias AS "alias!", a.id AS "agent_id!",
                          alternate.kind AS "kind!"
                     FROM agents.agents a
                     CROSS JOIN LATERAL (
                         SELECT a.name AS alias, 'name'::text AS kind
                         UNION ALL
                         SELECT p.alias, 'persona'::text AS kind
                           FROM UNNEST(a.persona_aliases) AS p(alias)
                         UNION ALL
                         SELECT n.alias, 'name_variant'::text AS kind
                           FROM UNNEST(a.name_variants) AS n(alias)
                     ) alternate
                    ORDER BY a.id, alternate.alias"#
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn upsert_persona_alias(
        &self,
        agent_id: i64,
        alias: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("upsert_persona_alias", async move {
            let mut tx = self.pool.begin().await?;

            sqlx::query!(
                "SELECT pg_advisory_xact_lock(
                     hashtextextended('agents.alias:' || LOWER($1), 0)
                 )",
                alias,
            )
            .execute(&mut *tx)
            .await?;

            let result = sqlx::query!(
                "WITH candidate AS (
                     SELECT $2::text AS alias
                      WHERE NOT EXISTS (
                          SELECT 1
                            FROM agents.agents named
                           WHERE LOWER(named.name) = LOWER($2)
                      )
                        AND NOT EXISTS (
                          SELECT 1
                            FROM agents.agents existing
                            CROSS JOIN LATERAL UNNEST(
                                existing.persona_aliases || existing.name_variants
                            ) AS known(alias)
                           WHERE LOWER(known.alias) = LOWER($2)
                      )
                 )
                 UPDATE agents.agents target
                    SET persona_aliases = ARRAY_APPEND(target.persona_aliases, candidate.alias)
                   FROM candidate
                  WHERE target.id = $1",
                agent_id,
                alias,
            )
            .execute(&mut *tx)
            .await?;
            let inserted = result.rows_affected() > 0;
            tx.commit().await?;
            Ok(inserted)
        })
        .await
    }
}

pub async fn find_or_create_agent_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    name: &str,
    definition_path: Option<&str>,
) -> Result<(Agent, i64), PersistError> {
    let row = sqlx::query_as!(
        super::crud::AgentRow,
        r#"WITH upserted AS (
                INSERT INTO agents.agents (name, definition_path)
                VALUES ($1, $2)
                ON CONFLICT (LOWER(name)) DO SELECT
                RETURNING id, name, definition_path, description, source_id,
                          created_at, updated_at
            )
            SELECT u.id AS "id!", u.name AS "name!", u.definition_path,
                   u.description, s.code AS "source!",
                   u.created_at AS "created_at!", u.updated_at AS "updated_at!"
              FROM upserted u
              JOIN agents.sources s ON s.id = u.source_id"#,
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
    use crate::store::persistence::test_support::TestDb;

    #[tokio::test]
    async fn existing_agent_lookup_preserves_the_tuple_and_definition() -> anyhow::Result<()> {
        let Some(factory) = TestDb::bootstrap().await? else {
            return Ok(());
        };
        let db = factory.acquire().await?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let name = format!("pg19-registry-{}", uuid::Uuid::new_v4());
        let mut first = handle.pool.begin().await?;
        let (_, id) = find_or_create_agent_in_tx(&mut first, &name, Some("original.md")).await?;
        first.commit().await?;
        let before: String =
            sqlx::query_scalar("SELECT xmin::text FROM agents.agents WHERE id = $1")
                .bind(id)
                .fetch_one(&handle.pool)
                .await?;
        let mut second = handle.pool.begin().await?;
        let (agent, found) =
            find_or_create_agent_in_tx(&mut second, &name, Some("ignored.md")).await?;
        second.commit().await?;
        let after: String =
            sqlx::query_scalar("SELECT xmin::text FROM agents.agents WHERE id = $1")
                .bind(id)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(found, id);
        assert_eq!(agent.definition_path.as_deref(), Some("original.md"));
        assert_eq!(after, before, "a lookup must not create a dead tuple");
        Ok(())
    }
}
