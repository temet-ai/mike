use std::collections::HashMap;

use serde_json::Value;

use super::super::super::{PersistError, PersistHandle};
use super::{CompositionRevision, CompositionRevisionRow, CompositionRow};
use crate::compose_prompt::PromptClassKey;
use crate::compose_prompt::decision::{Composition, CompositionMode};

impl PersistHandle {
    pub async fn load_prompt_compositions(
        &self,
    ) -> Result<HashMap<PromptClassKey, Composition>, PersistError> {
        crate::metrics::time_db("load_prompt_compositions", async {
            let rows = sqlx::query_as!(
                CompositionRow,
                r#"SELECT pc.id, pc.uuid, sp.prompt_sha256,
                          pc.label, pc.blocks, m.code AS "mode!", pc.lane,
                          pc.notes, pc.agent_name, pc.agent_definition_path,
                          pc.agent_id AS "agent_id?", pc.version, pc.created_at, pc.updated_at
                 FROM overrides.overrides pc
                 JOIN overrides.modes m ON m.id = pc.mode_id
                 JOIN system_prompts.system_prompts sp ON sp.id = pc.system_prompt_id"#,
            )
            .fetch_all(&self.pool)
            .await?;

            let mut out = HashMap::with_capacity(rows.len());
            for row in rows {
                let composition = Composition::try_from(row)?;
                match PromptClassKey::from_lane_and_agent_name(
                    &composition.lane,
                    composition.agent_name.as_deref(),
                ) {
                    Some(key) => {
                        out.insert(key, composition);
                    }
                    None => {
                        tracing::warn!(
                            composition_id = composition.id,
                            lane = %composition.lane,
                            agent_name = ?composition.agent_name,
                            "override row does not resolve to a prompt class key; skipped from composer cache"
                        );
                    }
                }
            }
            Ok(out)
        })
        .await
    }
    pub(crate) async fn fetch_composition_by_id(
        &self,
        id: i64,
    ) -> Result<Option<Composition>, PersistError> {
        crate::metrics::time_db("fetch_composition_by_id", async {
            let row = sqlx::query_as!(
                CompositionRow,
                r#"SELECT pc.id, pc.uuid, sp.prompt_sha256,
                          pc.label, pc.blocks, m.code AS "mode!", pc.lane,
                          pc.notes, pc.agent_name, pc.agent_definition_path,
                          pc.agent_id AS "agent_id?", pc.version, pc.created_at, pc.updated_at
                 FROM overrides.overrides pc
                 JOIN overrides.modes m ON m.id = pc.mode_id
                 JOIN system_prompts.system_prompts sp ON sp.id = pc.system_prompt_id
                 WHERE pc.id = $1"#,
                id,
            )
            .fetch_optional(&self.pool)
            .await?;

            match row {
                Some(r) => Ok(Some(Composition::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }
    pub(crate) async fn fetch_composition_by_prompt_sha256(
        &self,
        prompt_sha256: &str,
    ) -> Result<Option<Composition>, PersistError> {
        crate::metrics::time_db("fetch_composition_by_prompt_sha256", async {
            let row = sqlx::query_as!(
                CompositionRow,
                r#"SELECT pc.id, pc.uuid, sp.prompt_sha256,
                          pc.label, pc.blocks, m.code AS "mode!", pc.lane,
                          pc.notes, pc.agent_name, pc.agent_definition_path,
                          pc.agent_id AS "agent_id?", pc.version, pc.created_at, pc.updated_at
                 FROM overrides.overrides pc
                 JOIN overrides.modes m ON m.id = pc.mode_id
                 JOIN system_prompts.system_prompts sp ON sp.id = pc.system_prompt_id
                 WHERE sp.prompt_sha256 = $1
                 ORDER BY pc.id
                 LIMIT 1"#,
                prompt_sha256,
            )
            .fetch_optional(&self.pool)
            .await?;

            match row {
                Some(r) => Ok(Some(Composition::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }
    pub(crate) async fn list_composition_revisions(
        &self,
        composition_id: i64,
    ) -> Result<Vec<CompositionRevision>, PersistError> {
        crate::metrics::time_db("list_composition_revisions", async {
            let rows = sqlx::query_as!(
                CompositionRevisionRow,
                r#"SELECT pcr.id, pcr.uuid, pc.id AS "composition_id!",
                        pc.uuid AS "composition_uuid!", pcr.version, pcr.label,
                        pcr.blocks, m.code AS "mode!", pcr.notes, pcr.created_at
                 FROM overrides.revisions pcr
                 JOIN overrides.overrides pc ON pc.id = pcr.override_id
                 JOIN overrides.modes m ON m.id = pcr.mode_id
                 WHERE pc.id = $1
                 ORDER BY pcr.version DESC"#,
                composition_id,
            )
            .fetch_all(&self.pool)
            .await?;

            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let mode = CompositionMode::from_str(&row.mode).map_err(|err| {
                    PersistError::DataInvariant(format!(
                        "prompt_composition_revisions row id={}: {err}",
                        row.id
                    ))
                })?;
                let blocks: Vec<Value> = serde_json::from_value(row.blocks).map_err(|err| {
                    PersistError::DataInvariant(format!(
                        "prompt_composition_revisions row id={}: failed to decode blocks JSON: {err}",
                        row.id
                    ))
                })?;
                out.push(CompositionRevision {
                    id: row.id,
                    uuid: row.uuid,
                    composition_id: row.composition_id,
                    composition_uuid: row.composition_uuid,
                    version: row.version,
                    label: row.label,
                    blocks,
                    mode,
                    notes: row.notes,
                    created_at: row.created_at,
                });
            }
            Ok(out)
        })
        .await
    }
}
