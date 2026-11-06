use super::{SteererFields, SteererRow, SteererUpdate};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn list_steerers(&self) -> Result<Vec<SteererRow>, PersistError> {
        crate::metrics::time_db("list_steerers", async move {
            let rows = sqlx::query_as!(
                SteererRow,
                r#"SELECT
                       s.id          AS "id!",
                       s.uuid        AS "uuid!",
                       s.name        AS "name!",
                       s.display_name AS "display_name!",
                       s.description AS "description!",
                       s.agent_id    AS "agent_id!",
                       a.code        AS "authority!",
                       m.code        AS "mode!",
                       s.model       AS "model!",
                       s.priority    AS "priority!",
                       s.enabled     AS "enabled!",
                       s.config      AS "config!",
                       s.prompt,
                       s.kind         AS "kind!",
                       s.trigger_point AS "trigger_point!",
                       s.invocation   AS "invocation!",
                       s.pattern,
                       s.auto_enrol   AS "auto_enrol!",
                       s.project_id,
                       p.project_path AS "project_path?",
                       s.created_at  AS "created_at!",
                       s.updated_at  AS "updated_at!"
                   FROM steering.steerers s
                   JOIN steering.authorities a ON a.id = s.authority_id
                   JOIN steering.modes       m ON m.id = s.default_mode_id
                   LEFT JOIN projects.projects p ON p.id = s.project_id
                  WHERE s.deleted_at IS NULL
                   ORDER BY s.priority ASC, s.id ASC"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn fetch_steerer_by_id(
        &self,
        id: i64,
    ) -> Result<Option<SteererRow>, PersistError> {
        crate::metrics::time_db("fetch_steerer_by_id", async move {
            let row = sqlx::query_as!(
                SteererRow,
                r#"SELECT
                       s.id          AS "id!",
                       s.uuid        AS "uuid!",
                       s.name        AS "name!",
                       s.display_name AS "display_name!",
                       s.description AS "description!",
                       s.agent_id    AS "agent_id!",
                       a.code        AS "authority!",
                       m.code        AS "mode!",
                       s.model       AS "model!",
                       s.priority    AS "priority!",
                       s.enabled     AS "enabled!",
                       s.config      AS "config!",
                       s.prompt,
                       s.kind        AS "kind!",
                       s.trigger_point AS "trigger_point!",
                       s.invocation  AS "invocation!",
                       s.pattern,
                       s.auto_enrol  AS "auto_enrol!",
                       s.project_id,
                       p.project_path AS "project_path?",
                       s.created_at  AS "created_at!",
                       s.updated_at  AS "updated_at!"
                   FROM steering.steerers s
                   JOIN steering.authorities a ON a.id = s.authority_id
                   JOIN steering.modes       m ON m.id = s.default_mode_id
                   LEFT JOIN projects.projects p ON p.id = s.project_id
                  WHERE s.id = $1
                    AND s.deleted_at IS NULL"#,
                id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub(crate) async fn insert_steerer(
        &self,
        agent_id: i64,
        fields: SteererFields<'_>,
        source: Option<&str>,
    ) -> Result<(i64, uuid::Uuid), PersistError> {
        crate::metrics::time_db("insert_steerer", async move {
            let mut tx = self.pool.begin().await?;
            if let Some(source) = source {
                crate::store::persistence::change_log::set_change_source(&mut tx, source).await?;
            }
            let row = sqlx::query!(
                r#"INSERT INTO steering.steerers
                       (name, display_name, description, agent_id,
                        authority_id, default_mode_id,
                        model, priority, enabled, config, prompt,
                        trigger_point, invocation, pattern, project_id)
                   SELECT $1, $2, $3, $4,
                          a.id, m.id,
                          $7, $8, $9, $10, $11,
                          $12, $13, $14, $15
                     FROM steering.authorities a,
                          steering.modes       m
                    WHERE a.code = $5
                      AND m.code = $6
                   RETURNING id, uuid"#,
                fields.name,
                fields.display_name,
                fields.description,
                agent_id,
                fields.authority_code,
                fields.mode_code,
                fields.model,
                fields.priority,
                fields.enabled,
                fields.config,
                fields.prompt,
                fields.trigger_point,
                fields.invocation,
                fields.pattern,
                fields.project_id,
            )
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok((row.id, row.uuid))
        })
        .await
    }

    pub(crate) async fn insert_steerer_with_bootstrapped_agent(
        &self,
        fields: SteererFields<'_>,
        source: Option<&str>,
    ) -> Result<(i64, uuid::Uuid, i64), PersistError> {
        crate::metrics::time_db("insert_steerer_with_bootstrapped_agent", async move {
            let mut tx = self.pool.begin().await?;
            if let Some(source) = source {
                crate::store::persistence::change_log::set_change_source(&mut tx, source).await?;
            }

            let agent_row = sqlx::query!(
                r#"INSERT INTO agents.agents (name, description)
                   VALUES ($1, $2)
                   RETURNING id"#,
                fields.name,
                fields.description,
            )
            .fetch_one(&mut *tx)
            .await?;
            let agent_id = agent_row.id;

            let steerer_row = sqlx::query!(
                r#"INSERT INTO steering.steerers
                       (name, display_name, description, agent_id,
                        authority_id, default_mode_id,
                        model, priority, enabled, config, prompt,
                        trigger_point, invocation, pattern, project_id)
                   SELECT $1, $2, $3, $4,
                          a.id, m.id,
                          $7, $8, $9, $10, $11,
                          $12, $13, $14, $15
                     FROM steering.authorities a,
                          steering.modes       m
                    WHERE a.code = $5
                      AND m.code = $6
                   RETURNING id, uuid"#,
                fields.name,
                fields.display_name,
                fields.description,
                agent_id,
                fields.authority_code,
                fields.mode_code,
                fields.model,
                fields.priority,
                fields.enabled,
                fields.config,
                fields.prompt,
                fields.trigger_point,
                fields.invocation,
                fields.pattern,
                fields.project_id,
            )
            .fetch_one(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok((steerer_row.id, steerer_row.uuid, agent_id))
        })
        .await
    }

    pub(crate) async fn update_steerer(
        &self,
        fields: SteererUpdate<'_>,
        source: Option<&str>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("update_steerer", async move {
            let mut tx = self.pool.begin().await?;
            if let Some(source) = source {
                crate::store::persistence::change_log::set_change_source(&mut tx, source).await?;
            }
            let result = sqlx::query!(
                "UPDATE steering.steerers s
                    SET display_name    = $2,
                        description     = $3,
                        enabled         = $4,
                        authority_id    = a.id,
                        default_mode_id = m.id,
                        model           = $7,
                        priority        = $8,
                        config          = s.config || $9::jsonb,
                        prompt          = $10,
                        trigger_point   = $11,
                        invocation      = $12,
                        pattern         = $13,
                        project_id      = $14,
                        updated_at      = NOW()
                   FROM steering.authorities a,
                        steering.modes       m
                  WHERE a.code = $5
                    AND m.code = $6
                    AND s.id   = $1",
                fields.id,
                fields.display_name,
                fields.description,
                fields.enabled,
                fields.authority_code,
                fields.mode_code,
                fields.model,
                fields.priority,
                fields.config,
                fields.prompt,
                fields.trigger_point,
                fields.invocation,
                fields.pattern,
                fields.project_id,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub(crate) async fn delete_steerer(
        &self,
        id: i64,
        source: Option<&str>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("delete_steerer", async move {
            let mut tx = self.pool.begin().await?;
            if let Some(source) = source {
                crate::store::persistence::change_log::set_change_source(&mut tx, source).await?;
            }
            let result = sqlx::query!(
                "UPDATE steering.steerers
                    SET deleted_at = NOW(),
                        enabled    = false,
                        updated_at = NOW()
                  WHERE id = $1
                    AND deleted_at IS NULL",
                id,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }
}
