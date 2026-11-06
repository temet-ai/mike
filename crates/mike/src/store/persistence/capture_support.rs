use super::id_resolvers::{resolve_or_create_agent_id, resolve_tool_use_id};
use super::{PersistError, PersistHandle};
use super::{messages, rehydrate};

pub(crate) struct StreamBlockRow<'a> {
    pub(crate) request_id: i64,
    pub(crate) seq_ord: i32,
    pub(crate) block_type: &'a str,
    pub(crate) content: &'a str,
    pub(crate) signature: Option<&'a str>,
    pub(crate) wire_tool_use_id: Option<&'a str>,
    pub(crate) content_sha256: &'a str,
}

pub(crate) fn operation_for_tool(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "Read" => Some("read"),
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => Some("write"),
        "Bash" => Some("exec"),
        "Grep" | "Glob" | "WebSearch" => Some("search"),
        "WebFetch" => Some("fetch"),
        "Agent" => Some("dispatch"),
        _ => None,
    }
}

const INSERT_STREAM_BLOCK_SQL: &str = "WITH owner AS (
         SELECT t.id, t.closed_at IS NOT NULL AS closed
           FROM requests.requests r
           JOIN memory.operator_turns t ON t.id = r.operator_turn_id
          WHERE r.id = $1
            FOR SHARE OF t
     )
     INSERT INTO responses.stream_blocks
          (request_id, session_id, block_type_id, seq_ord, content,
           signature, wire_tool_use_id, content_sha256, byte_size)
     SELECT $1, r.session_id, sbt.id, $2, $3, $4, $5, $6, $7
       FROM requests.requests r, responses.stream_block_types sbt
      WHERE r.id = $1
        AND sbt.code = $8
     ON CONFLICT (request_id, seq_ord) DO NOTHING
     RETURNING (SELECT o.id FROM owner o WHERE o.closed)";

impl PersistHandle {
    pub(crate) async fn insert_stream_block_row(
        &self,
        row: StreamBlockRow<'_>,
    ) -> Result<(), PersistError> {
        let byte_size = row.content.len() as i32;
        crate::metrics::time_db("insert_stream_block_row", async {
            let closed_turn: Option<Option<i64>> = sqlx::query_scalar(INSERT_STREAM_BLOCK_SQL)
                .bind(row.request_id)
                .bind(row.seq_ord)
                .bind(row.content)
                .bind(row.signature)
                .bind(row.wire_tool_use_id)
                .bind(row.content_sha256)
                .bind(byte_size)
                .bind(row.block_type)
                .fetch_optional(&self.pool)
                .await?;
            if let Some(Some(turn_id)) = closed_turn {
                let mut tx = self.pool.begin().await?;
                super::turn_lifecycle::materialize_turn_aggregates(&mut tx, turn_id).await?;
                tx.commit().await?;
            }
            Ok(())
        })
        .await
    }
    pub(crate) async fn insert_tool_use_row(
        &self,
        request_id: i64,
        request_created_at: chrono::DateTime<chrono::Utc>,
        ord: i32,
        tc: &crate::types::ToolCall,
    ) -> Result<(), PersistError> {
        let input_value: Option<serde_json::Value> = if tc.input_json.is_empty() {
            None
        } else {
            serde_json::from_str(&tc.input_json).ok()
        };
        let file_path: Option<String> = input_value
            .as_ref()
            .and_then(|v| v.get("file_path"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let operation: Option<&str> = operation_for_tool(&tc.name);
        crate::metrics::time_db("insert_tool_use_row", async {
            sqlx::query!(
                "INSERT INTO tools.uses
                     (wire_tool_use_id, request_id, ord, tool_name, input,
                      file_path, operation, created_at, session_id)
                 SELECT $1, $2, $3, $4, $5, $6, $7, $8, r.session_id
                 FROM requests.requests r
                 WHERE r.id = $2
                 ON CONFLICT (wire_tool_use_id) DO NOTHING",
                tc.tool_id,
                request_id,
                ord,
                tc.name,
                input_value as Option<serde_json::Value>,
                file_path,
                operation,
                request_created_at,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub(crate) async fn insert_agent_dispatch_row(
        &self,
        tool_use_id: &str,
        agent_name: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("insert_agent_dispatch_row", async {
            let agent_id = resolve_or_create_agent_id(&self.pool, agent_name).await?;
            let Some(tool_use_db_id) = resolve_tool_use_id(&self.pool, tool_use_id).await? else {
                return Ok(());
            };
            sqlx::query!(
                "INSERT INTO agents.dispatches (tool_use_id, agent_id)
                 SELECT $1, $2
                 WHERE NOT EXISTS (
                     SELECT 1 FROM agents.dispatches WHERE tool_use_id = $1
                 )",
                tool_use_db_id,
                agent_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub(crate) async fn fetch_accumulated_text_for_request(
        &self,
        request_id: i64,
    ) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db(
            "fetch_accumulated_text_for_request",
            rehydrate::fetch_accumulated_text_for_request(&self.pool, request_id),
        )
        .await
    }
    pub(crate) async fn fetch_stream_blocks_for_request(
        &self,
        request_id: i64,
    ) -> Result<Vec<crate::store::models::PersistedStreamBlockRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_stream_blocks_for_request",
            rehydrate::fetch_stream_blocks_for_request(&self.pool, request_id),
        )
        .await
    }
    pub(crate) async fn fetch_request_deltas_by_id(
        &self,
        request_id: i64,
        session_db_id: i64,
    ) -> Result<Vec<messages::PersistedDeltaRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_request_deltas_by_id",
            messages::fetch_request_deltas_by_id(&self.pool, request_id, session_db_id),
        )
        .await
    }
    pub async fn resolve_session_db_id(
        &self,
        session_uuid: &str,
    ) -> Result<Option<i64>, PersistError> {
        let Ok(session_uuid) = session_uuid.parse::<uuid::Uuid>() else {
            return Ok(None);
        };
        self.session_row_id(session_uuid).await
    }
    pub(crate) async fn mark_session_prompt_altered(
        &self,
        session_id: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("mark_session_prompt_altered", async move {
            let Some(session_db_id) = self.resolve_session_db_id(session_id).await? else {
                return Ok(());
            };
            sqlx::query!(
                "UPDATE system_prompts.session_prompts
                 SET altered = true
                 WHERE session_id = $1
                   AND lane = 'main'
                   AND altered = false",
                session_db_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub(crate) async fn mark_session_prompt_alteration_used(
        &self,
        session_id: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("mark_session_prompt_alteration_used", async move {
            let Some(session_db_id) = self.resolve_session_db_id(session_id).await? else {
                return Ok(());
            };
            sqlx::query!(
                "UPDATE system_prompts.session_prompts
                 SET alteration_used = true
                 WHERE session_id = $1
                   AND lane = 'main'
                   AND alteration_used = false",
                session_db_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
    pub(crate) async fn fetch_predecessor_request_id(
        &self,
        session_db_id: i64,
        created_at: chrono::DateTime<chrono::Utc>,
        agent_instance_id: Option<i64>,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db(
            "fetch_predecessor_request_id",
            rehydrate::fetch_predecessor_request_id(
                &self.pool,
                session_db_id,
                created_at,
                agent_instance_id,
            ),
        )
        .await
    }
    pub(crate) async fn fetch_request_number(
        &self,
        request_id: u64,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db(
            "fetch_request_number",
            rehydrate::fetch_request_number(&self.pool, request_id),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_for_tool_canonical_map() {
        const CASES: &[(&str, Option<&str>)] = &[
            ("Read", Some("read")),
            ("Write", Some("write")),
            ("Edit", Some("write")),
            ("MultiEdit", Some("write")),
            ("NotebookEdit", Some("write")),
            ("Bash", Some("exec")),
            ("Grep", Some("search")),
            ("Glob", Some("search")),
            ("WebSearch", Some("search")),
            ("WebFetch", Some("fetch")),
            ("Agent", Some("dispatch")),
            ("SomeUnknownTool", None),
            ("mcp__my_server__my_tool", None),
            ("TaskCreate", None),
            ("TodoWrite", None),
            ("", None),
        ];
        for (tool_name, expected) in CASES {
            assert_eq!(
                operation_for_tool(tool_name),
                *expected,
                "operation_for_tool({tool_name:?})"
            );
        }
    }
}
