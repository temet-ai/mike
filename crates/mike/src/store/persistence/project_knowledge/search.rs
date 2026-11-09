use serde::Deserialize;
use serde_json::Value;

use super::invalid;
use crate::store::persistence::recall_embedding::set_vector_scan_limits;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Default, Deserialize)]
pub struct KnowledgeQuery {
    pub project: String,
    #[serde(default)]
    pub query: String,
    pub kind: Option<String>,
    #[serde(default)]
    pub include_historical: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn search_page(
    project_uuid: uuid::Uuid,
    hybrid: bool,
    rows: Vec<Value>,
    limit: i64,
    offset: i64,
) -> Value {
    let has_more = rows.len() > limit as usize;
    let items: Vec<Value> = rows.into_iter().take(limit as usize).collect();
    serde_json::json!({"project_uuid": project_uuid,
        "retrieval_path": if hybrid { "hybrid" } else { "lexical" }, "items": items, "has_more": has_more,
        "next_offset": if has_more { Some(offset + limit) } else { None },
        "read_tool": "read_project_knowledge"})
}

impl PersistHandle {
    pub async fn search_project_knowledge(
        &self,
        input: &KnowledgeQuery,
    ) -> Result<Value, PersistError> {
        self.search_project_knowledge_hybrid(input, None).await
    }

    pub(crate) async fn search_project_knowledge_hybrid(
        &self,
        input: &KnowledgeQuery,
        vector: Option<&(String, String, String)>,
    ) -> Result<Value, PersistError> {
        let project = self.resolve_knowledge_project(&input.project).await?;
        let limit = input.limit.unwrap_or(20).clamp(1, 100);
        let offset = input.offset.unwrap_or(0).max(0);
        let rows = self
            .knowledge_search_rows(project.id, input, limit, offset, vector)
            .await?;
        Ok(search_page(
            project.project_uuid,
            vector.is_some(),
            rows,
            limit,
            offset,
        ))
    }

    async fn knowledge_search_rows(
        &self,
        project_id: i64,
        input: &KnowledgeQuery,
        limit: i64,
        offset: i64,
        vector: Option<&(String, String, String)>,
    ) -> Result<Vec<Value>, PersistError> {
        let mut tx = self.pool().begin().await?;
        if vector.is_some() {
            set_vector_scan_limits(&mut tx).await?;
        }
        let rows = sqlx::query_file_scalar!(
            "src/store/persistence/project_knowledge/search.sql",
            project_id,
            input.query,
            input.kind,
            input.include_historical,
            limit + 1,
            offset,
            vector.map(|v| v.0.as_str()),
            vector.map(|v| v.1.as_str()),
            vector.map(|v| v.2.as_str()),
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows)
    }

    pub async fn read_project_knowledge(
        &self,
        project: &str,
        reference: &str,
    ) -> Result<Value, PersistError> {
        let project = self.resolve_knowledge_project(project).await?;
        let (kind, key) = reference
            .split_once(':')
            .ok_or_else(|| invalid("Invalid knowledge reference"))?;
        let number = key.parse::<i64>().ok();
        let uuid = key.parse::<uuid::Uuid>().ok();
        if !matches!(
            kind,
            "artifact" | "lesson" | "note" | "turn" | "promise" | "thread" | "work_thread"
        ) {
            return Err(invalid("Unknown knowledge source kind"));
        }
        let value: Option<Value> = sqlx::query_scalar(include_str!("read.sql"))
            .bind(project.id)
            .bind(kind)
            .bind(number)
            .bind(uuid)
            .bind(key)
            .fetch_optional(self.pool())
            .await?;
        value.ok_or_else(|| invalid("Knowledge reference not found in this project"))
    }
}
