use serde::Deserialize;
use serde_json::Value;

use super::invalid;
use crate::analysis::embeddings::{ScoreFloor, score_floor_for};
use crate::store::persistence::recall_embedding::set_vector_scan_limits;
use crate::store::persistence::{PersistError, PersistHandle};

const READABLE_KINDS: [&str; 14] = [
    "launch",
    "artifact",
    "lesson",
    "note",
    "turn",
    "promise",
    "thread",
    "work_thread",
    "thread_entry",
    "question_answer",
    "unmet_ask",
    "correction",
    "learning",
    "mail",
];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SemanticCut {
    pub floor: Option<f64>,
    pub window: Option<f64>,
}

pub(crate) fn cut_for(model_name: &str, cut: SemanticCut) -> ScoreFloor {
    let model = score_floor_for(model_name);
    ScoreFloor {
        floor: cut.floor.unwrap_or(model.floor),
        window: cut.window.unwrap_or(model.window),
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct KnowledgeQuery {
    #[serde(skip)]
    pub semantic_cut: SemanticCut,
    #[serde(skip)]
    pub any_term: bool,
    pub project: String,
    #[serde(default)]
    pub query: String,
    pub kind: Option<String>,
    #[serde(default)]
    pub include_historical: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

struct QueryVector<'a> {
    literal: Option<&'a str>,
    model_key: Option<&'a str>,
    model_name: Option<&'a str>,
    cut: ScoreFloor,
}

impl<'a> QueryVector<'a> {
    fn from(vector: Option<&'a (String, String, String)>, cut: SemanticCut) -> Self {
        match vector {
            Some((literal, key, name)) => Self {
                literal: Some(literal),
                model_key: Some(key),
                model_name: Some(name),
                cut: cut_for(name, cut),
            },
            None => Self {
                literal: None,
                model_key: None,
                model_name: None,
                cut: ScoreFloor::NO_CUT,
            },
        }
    }
}

async fn fetch_hits(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    (project_id, input): (i64, &KnowledgeQuery),
    (limit, offset): (i64, i64),
    bound: &QueryVector<'_>,
) -> Result<Vec<Value>, PersistError> {
    Ok(sqlx::query_file_scalar!(
        "src/store/persistence/project_knowledge/search.sql",
        project_id,
        input.query,
        input.kind,
        input.include_historical,
        limit + 1,
        offset,
        bound.literal,
        bound.model_key,
        bound.model_name,
        bound.cut.floor,
        bound.cut.window,
        input.any_term,
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub(crate) fn search_page(
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

    pub(crate) async fn knowledge_candidate_rows(
        &self,
        project_id: i64,
        input: &KnowledgeQuery,
        count: i64,
        vector: Option<&(String, String, String)>,
    ) -> Result<Vec<Value>, PersistError> {
        self.knowledge_search_rows(project_id, input, count, 0, vector)
            .await
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
        let bound = QueryVector::from(vector, input.semantic_cut);
        let rows = fetch_hits(&mut tx, (project_id, input), (limit, offset), &bound).await?;
        tx.commit().await?;
        Ok(rows)
    }

    pub async fn read_project_knowledge(
        &self,
        project: Option<&str>,
        reference: &str,
    ) -> Result<Value, PersistError> {
        let project_id = match project.filter(|p| !p.trim().is_empty()) {
            Some(selector) => Some(self.resolve_knowledge_project(selector).await?.id),
            None => None,
        };
        let (kind, key) = reference
            .split_once(':')
            .ok_or_else(|| invalid("Invalid knowledge reference"))?;
        if !READABLE_KINDS.contains(&kind) {
            return Err(invalid("Unknown knowledge source kind"));
        }
        let mut found: Vec<Value> = sqlx::query_scalar(include_str!("read.sql"))
            .bind(project_id)
            .bind(kind)
            .bind(key.parse::<i64>().ok())
            .bind(key.parse::<uuid::Uuid>().ok())
            .bind(key)
            .fetch_all(self.pool())
            .await?;
        match (found.len(), project_id) {
            (0, Some(_)) => Err(invalid("Knowledge reference not found in this project")),
            (0, None) => Err(invalid("Knowledge reference not found")),
            (1, _) => Ok(found.remove(0)),
            _ => Err(invalid(
                "Knowledge reference matches several projects; pass project",
            )),
        }
    }
}
