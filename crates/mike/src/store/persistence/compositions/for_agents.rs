//! The compositions each agent has, newest first (agents' evidence reads them).

use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn list_compositions_for_agent(
        &self,
        agent_id: i64,
    ) -> Result<Vec<crate::wire::composition::Composition>, PersistError> {
        let mut by_agent = self.list_compositions_for_agents(&[agent_id]).await?;
        Ok(by_agent.remove(&agent_id).unwrap_or_default())
    }

    pub async fn list_compositions_for_agents(
        &self,
        agent_ids: &[i64],
    ) -> Result<
        std::collections::HashMap<i64, Vec<crate::wire::composition::Composition>>,
        PersistError,
    > {
        if agent_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        crate::metrics::time_db("list_compositions_for_agents", async move {
            let rows = sqlx::query_as!(
                crate::store::persistence::compositions::CompositionRow,
                r#"SELECT pc.id, pc.uuid, pc.prompt_sha256 AS "prompt_sha256!",
                          pc.label, pc.blocks, m.code AS "mode!", pc.lane,
                          pc.notes, pc.agent_id AS "agent_id?", pc.agent_name,
                          pc.agent_definition_path, pc.version, pc.created_at, pc.updated_at
                     FROM overrides.overrides pc
                     JOIN overrides.modes m ON m.id = pc.mode_id
                    WHERE pc.agent_id = ANY($1)
                    ORDER BY pc.agent_id, pc.created_at DESC"#,
                agent_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            let mut out: std::collections::HashMap<
                i64,
                Vec<crate::wire::composition::Composition>,
            > = std::collections::HashMap::new();
            for row in rows {
                let composition = crate::wire::composition::Composition::try_from(row)?;
                if let Some(agent_id) = composition.agent_id {
                    out.entry(agent_id).or_default().push(composition);
                }
            }
            Ok(out)
        })
        .await
    }
}
