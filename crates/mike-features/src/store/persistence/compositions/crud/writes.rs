use serde_json::Value;

use super::super::super::PersistError;
use super::super::super::agents::find_or_create_agent_in_tx;
use super::{
    CompositionRow, DeleteOutcome, NewCompositionParams, OwnedNewComposition,
    PatchCompositionParams,
};
use crate::compose_prompt::PromptClassKey;
use crate::wire::composition::{Composition, CompositionMode};

async fn resolve_new_composition_agent_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    agent_name: Option<&str>,
    agent_definition_path: Option<&str>,
) -> Result<Option<i64>, PersistError> {
    match agent_name {
        Some(name) if !name.is_empty() => {
            let (_agent, db_id) =
                find_or_create_agent_in_tx(tx, name, agent_definition_path).await?;
            Ok(Some(db_id))
        }
        _ => Ok(None),
    }
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn create_composition(
        self,
        params: NewCompositionParams<'_>,
    ) -> Result<Composition, PersistError> {
        let NewCompositionParams {
            prompt_sha256,
            label,
            blocks,
            mode,
            agent_name,
            agent_definition_path,
        } = params;

        let mode_str = mode.unwrap_or(CompositionMode::Observe).as_str();
        let lane = match agent_name {
            Some(name) if !name.is_empty() => PromptClassKey::Subagent(name.to_string()),
            _ => PromptClassKey::Main,
        }
        .lane();
        let owned = OwnedNewComposition {
            prompt_sha256: prompt_sha256.to_owned(),
            label: label.to_owned(),
            blocks: blocks.clone(),
            mode_str,
            agent_name: agent_name.map(str::to_owned),
            agent_definition_path: agent_definition_path.map(str::to_owned),
            lane,
        };

        crate::metrics::time_db(
            "create_composition",
            self.insert_composition_and_revision(owned),
        )
        .await
    }

    async fn insert_composition_and_revision(
        self,
        f: OwnedNewComposition,
    ) -> Result<Composition, PersistError> {
        let mut tx = self.pool.begin().await?;

        let system_prompt_db_id =
            Self::register_system_prompt_in_tx(&mut tx, &f.prompt_sha256, &f.blocks).await?;
        let agent_db_id = resolve_new_composition_agent_id(
            &mut tx,
            f.agent_name.as_deref(),
            f.agent_definition_path.as_deref(),
        )
        .await?;

        let row = Self::insert_composition_row_in_tx(&mut tx, &f, agent_db_id, system_prompt_db_id)
            .await?;
        let comp = Composition::try_from(row)?;
        Self::insert_composition_revision_in_tx(&mut tx, &comp).await?;

        tx.commit().await?;
        Ok(comp)
    }

    async fn register_system_prompt_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        prompt_sha256: &str,
        blocks: &Value,
    ) -> Result<i64, PersistError> {
        let id = sqlx::query_scalar!(
            r#"WITH known_prompt AS (
                   SELECT sp.id
                     FROM system_prompts.system_prompts sp
                    WHERE sp.prompt_sha256 = $1
                    ORDER BY sp.id
                    LIMIT 1
               ),
               new_prompt AS (
                   INSERT INTO system_prompts.system_prompts
                          (prompt_sha256, blocks, first_seen_at, last_seen_at)
                   SELECT $1, $2, NOW(), NOW()
                    WHERE NOT EXISTS (SELECT 1 FROM known_prompt)
                   RETURNING id
               )
               SELECT id AS "id!" FROM new_prompt
               UNION ALL
               SELECT id AS "id!" FROM known_prompt"#,
            prompt_sha256,
            blocks,
        )
        .fetch_one(&mut **tx)
        .await?;
        Ok(id)
    }

    async fn insert_composition_row_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        f: &OwnedNewComposition,
        agent_db_id: Option<i64>,
        system_prompt_db_id: i64,
    ) -> Result<CompositionRow, PersistError> {
        let row = sqlx::query_as!(
            CompositionRow,
            r#"WITH ins AS (
                     INSERT INTO overrides.overrides
                         (prompt_sha256, label, blocks, mode_id,
                          agent_name, agent_definition_path, agent_id,
                          system_prompt_id, lane)
                     SELECT $1, $2, $3, m.id, $5, $6, $7, $8, $9
                     FROM overrides.modes m
                     WHERE m.code = $4
                     RETURNING id, uuid, prompt_sha256, label, blocks,
                               mode_id, lane, notes, agent_name, agent_definition_path,
                               agent_id, version, created_at, updated_at
                 )
                 SELECT ins.id, ins.uuid, ins.prompt_sha256 AS "prompt_sha256!",
                        ins.label, ins.blocks, m.code AS "mode!", ins.lane,
                        ins.notes, ins.agent_name, ins.agent_definition_path,
                        ins.agent_id AS "agent_id?",
                        ins.version, ins.created_at, ins.updated_at
                 FROM ins
                 JOIN overrides.modes m ON m.id = ins.mode_id"#,
            &f.prompt_sha256,
            &f.label,
            &f.blocks,
            f.mode_str,
            f.agent_name.as_deref(),
            f.agent_definition_path.as_deref(),
            agent_db_id,
            system_prompt_db_id,
            f.lane,
        )
        .fetch_one(&mut **tx)
        .await?;
        Ok(row)
    }

    async fn insert_composition_revision_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        comp: &Composition,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "INSERT INTO overrides.revisions
                    (override_id, version, label, blocks, mode_id, notes)
                 SELECT $1, $2, $3, $4, m.id, $6
                 FROM overrides.modes m
                 WHERE m.code = $5",
            comp.id,
            comp.version,
            &comp.label,
            serde_json::Value::Array(comp.blocks.clone()),
            comp.mode.as_str(),
            comp.notes.as_deref(),
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
    pub async fn patch_composition(
        self,
        id: i64,
        params: PatchCompositionParams<'_>,
    ) -> Result<Option<Composition>, PersistError> {
        let label = params.label.map(str::to_owned);
        let blocks = params.blocks.cloned();
        let mode = params.mode;
        let notes_patch = params.notes.map(|o| o.map(str::to_owned));
        let agent_name_patch = params.agent_name.map(|o| o.map(str::to_owned));
        let agent_definition_path_patch =
            params.agent_definition_path.map(|o| o.map(str::to_owned));

        crate::metrics::time_db("patch_composition", async move {
            let notes_set = notes_patch.is_some();
            let notes_value: Option<String> = notes_patch.flatten();
            let agent_name_set = agent_name_patch.is_some();
            let agent_name_value: Option<String> = agent_name_patch.flatten();
            let agent_definition_path_set = agent_definition_path_patch.is_some();
            let agent_definition_path_value: Option<String> = agent_definition_path_patch.flatten();
            let mode_str = mode.map(|m| m.as_str());

            let mut tx = self.pool.begin().await?;

            let agent_id_resolution: Option<Option<i64>> = match &agent_name_value {
                None if !agent_name_set => None,
                None => Some(None),
                Some(name) if name.is_empty() => Some(None),
                Some(name) => {
                    let path: Option<&str> = agent_definition_path_value.as_deref();
                    let (_agent, db_id) = find_or_create_agent_in_tx(&mut tx, name, path).await?;
                    Some(Some(db_id))
                }
            };
            let agent_id_set = agent_id_resolution.is_some();
            let agent_id_value: Option<i64> = agent_id_resolution.flatten();

            let row = sqlx::query_as!(
                CompositionRow,
                r#"WITH upd AS (
                     UPDATE overrides.overrides
                     SET label                 = COALESCE($2, label),
                         blocks                = COALESCE($3, blocks),
                         mode_id               = COALESCE(
                             (SELECT id FROM overrides.modes WHERE code = $4 LIMIT 1),
                             mode_id),
                         notes                 = CASE WHEN $5 THEN $6 ELSE notes END,
                         agent_name            = CASE WHEN $7 THEN $8 ELSE agent_name END,
                         agent_definition_path = CASE WHEN $9 THEN $10 ELSE agent_definition_path END,
                         agent_id              = CASE WHEN $11 THEN $12 ELSE agent_id END,
                         version    = version + 1,
                         updated_at = NOW()
                     WHERE id = $1
                     RETURNING id, uuid, prompt_sha256, label, blocks,
                               mode_id, lane, notes, agent_name, agent_definition_path,
                               agent_id, version, created_at, updated_at
                 )
                 SELECT upd.id, upd.uuid, upd.prompt_sha256 AS "prompt_sha256!",
                        upd.label, upd.blocks, m.code AS "mode!", upd.lane,
                        upd.notes, upd.agent_name, upd.agent_definition_path,
                        upd.agent_id AS "agent_id?",
                        upd.version, upd.created_at, upd.updated_at
                 FROM upd
                 JOIN overrides.modes m ON m.id = upd.mode_id"#,
                id,
                label.as_deref(),
                blocks.as_ref(),
                mode_str,
                notes_set,
                notes_value.as_deref(),
                agent_name_set,
                agent_name_value.as_deref(),
                agent_definition_path_set,
                agent_definition_path_value.as_deref(),
                agent_id_set,
                agent_id_value,
            )
            .fetch_optional(&mut *tx)
            .await?;

            let Some(r) = row else {
                tx.rollback().await?;
                return Ok(None);
            };
            let comp = Composition::try_from(r)?;

            sqlx::query!(
                "INSERT INTO overrides.revisions
                    (override_id, version, label, blocks, mode_id, notes)
                 SELECT $1, $2, $3, $4, m.id, $6
                 FROM overrides.modes m
                 WHERE m.code = $5",
                comp.id,
                comp.version,
                &comp.label,
                serde_json::Value::Array(comp.blocks.clone()),
                comp.mode.as_str(),
                comp.notes.as_deref(),
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(Some(comp))
        })
        .await
    }
    pub async fn delete_composition(self, id: i64) -> Result<DeleteOutcome, PersistError> {
        crate::metrics::time_db("delete_composition", async {
            let mut tx = self.pool.begin().await?;
            let referenced: i64 = sqlx::query_scalar!(
                "SELECT COUNT(*)::BIGINT
                   FROM requests.system_prompts
                  WHERE override_id = $1",
                id,
            )
            .fetch_one(&mut *tx)
            .await?
            .unwrap_or(0);
            if referenced > 0 {
                tx.rollback().await?;
                return Ok(DeleteOutcome::StillReferenced { count: referenced });
            }
            let result = sqlx::query!("DELETE FROM overrides.overrides WHERE id = $1", id)
                .execute(&mut *tx)
                .await?;
            let outcome = if result.rows_affected() > 0 {
                DeleteOutcome::Removed
            } else {
                DeleteOutcome::NotFound
            };
            tx.commit().await?;
            Ok(outcome)
        })
        .await
    }
}
