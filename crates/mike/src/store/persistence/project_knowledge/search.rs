use serde::Deserialize;
use serde_json::Value;

use super::window::SearchWindow;
use super::{QueryEmbedding, invalid};
use crate::analysis::embeddings::{ScoreFloor, score_floor_for};
use crate::search_ranking::identifiers::query_identifiers;
use crate::store::persistence::recall_embedding::set_vector_scan_limits;
use crate::store::persistence::{PersistError, PersistHandle};

const READABLE_KINDS: [&str; 15] = [
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
    "session",
    "mail",
];

pub(crate) const REFERENCE_NOT_IN_PROJECT: &str = "Knowledge reference not found in this project";

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
    #[serde(skip)]
    pub recent: bool,
    #[serde(skip)]
    pub caller_session: Option<String>,
    pub project: String,
    #[serde(default)]
    pub query: String,
    pub kind: Option<String>,
    #[serde(default)]
    pub include_historical: bool,
    #[serde(default)]
    pub include_own_turn: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub thread: Option<String>,
}

impl KnowledgeQuery {
    pub(crate) fn window(&self) -> Result<SearchWindow, String> {
        SearchWindow::parse(self.since.as_deref(), self.until.as_deref())
    }

    fn thread_slug(&self) -> Option<&str> {
        self.thread
            .as_deref()
            .map(str::trim)
            .filter(|slug| !slug.is_empty())
    }
}

struct QueryVector<'a> {
    literal: Option<&'a str>,
    model_keys: Option<Vec<String>>,
    model_name: Option<&'a str>,
    cut: ScoreFloor,
    nearest_first: bool,
}

impl<'a> QueryVector<'a> {
    fn cut_bounds(&self) -> [f64; 2] {
        [self.cut.floor, self.cut.window]
    }

    fn from(vector: Option<&'a QueryEmbedding>, cut: SemanticCut, nearest_first: bool) -> Self {
        match vector {
            Some((literal, keys, name)) => Self {
                literal: Some(literal),
                model_keys: Some(keys.accepted()),
                model_name: Some(name),
                cut: cut_for(name, cut),
                nearest_first,
            },
            None => Self {
                literal: None,
                model_keys: None,
                model_name: None,
                cut: ScoreFloor::NO_CUT,
                nearest_first: false,
            },
        }
    }
}

struct CallScope {
    caller: Option<uuid::Uuid>,
    identifiers: Vec<String>,
    thread: Option<String>,
    window: SearchWindow,
}

impl CallScope {
    fn of(input: &KnowledgeQuery, window: SearchWindow) -> Self {
        Self {
            thread: input.thread_slug().map(str::to_owned),
            window,
            identifiers: query_identifiers(&input.query),
            caller: input
                .caller_session
                .as_deref()
                .filter(|_| !input.include_own_turn)
                .and_then(|session| uuid::Uuid::parse_str(session.trim()).ok()),
        }
    }
}

fn fetch_hits<'a>(
    tx: &'a mut sqlx::Transaction<'_, sqlx::Postgres>,
    (project_id, input): (i64, &'a KnowledgeQuery),
    (limit, offset): (i64, i64),
    (bound, scope): (&'a QueryVector<'_>, &'a CallScope),
) -> impl std::future::Future<Output = Result<Vec<Value>, sqlx::Error>> + 'a {
    sqlx::query_scalar::<_, Value>(include_str!("search.sql"))
        .persistent(false)
        .bind(project_id)
        .bind(&input.query)
        .bind(&input.kind)
        .bind(input.include_historical)
        .bind(limit)
        .bind(offset)
        .bind(bound.literal)
        .bind(bound.model_keys.as_deref())
        .bind(bound.model_name)
        .bind(bound.cut_bounds().to_vec())
        .bind(vec![input.any_term, bound.nearest_first])
        .bind(scope.caller)
        .bind(&scope.identifiers)
        .bind(scope.thread.as_deref())
        .bind(scope.window.since)
        .bind(scope.window.until)
        .fetch_all(&mut **tx)
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

fn with_correction(mut item: Value, corrected: Option<String>) -> Value {
    if let (Some(full), Some(fields)) = (corrected, item.as_object_mut()) {
        fields.insert("reference_corrected".to_owned(), Value::String(full));
    }
    item
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
        vector: Option<&QueryEmbedding>,
    ) -> Result<Value, PersistError> {
        let project = self.resolve_knowledge_project(&input.project).await?;
        self.search_resolved_knowledge(&project, input, vector)
            .await
    }

    pub(crate) async fn search_resolved_knowledge(
        &self,
        project: &super::ProjectIdentity,
        input: &KnowledgeQuery,
        vector: Option<&QueryEmbedding>,
    ) -> Result<Value, PersistError> {
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
        vector: Option<&QueryEmbedding>,
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
        vector: Option<&QueryEmbedding>,
    ) -> Result<Vec<Value>, PersistError> {
        let window = input.window().map_err(|refusal| invalid(&refusal))?;
        if let Some(slug) = input.thread_slug() {
            self.require_work_thread(project_id, slug).await?;
        }
        let nearest_first = match vector {
            Some((_, keys, _)) => self.knowledge_nearest_first(project_id, keys).await?,
            None => false,
        };
        let mut tx = self.pool().begin().await?;
        if vector.is_some() {
            set_vector_scan_limits(&mut tx).await?;
        }
        let bound = QueryVector::from(vector, input.semantic_cut, nearest_first);
        let scope = CallScope::of(input, window);
        let rows = crate::metrics::time_db(
            "knowledge_search",
            fetch_hits(
                &mut tx,
                (project_id, input),
                (limit + 1, offset),
                (&bound, &scope),
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(rows)
    }

    async fn require_work_thread(&self, project_id: i64, slug: &str) -> Result<(), PersistError> {
        let known: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM work.threads WHERE project_id = $1 AND slug = $2)",
        )
        .bind(project_id)
        .bind(slug)
        .fetch_one(self.pool())
        .await?;
        if known {
            return Ok(());
        }
        Err(invalid(&format!(
            "thread {slug:?} is not a work thread of this project; list them with work_thread_list and pass the slug"
        )))
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
        let settled = self
            .settle_reference(project_id, reference, &READABLE_KINDS)
            .await?;
        let key = settled.key.as_str();
        let mut found: Vec<Value> = sqlx::query_scalar(include_str!("read.sql"))
            .bind(project_id)
            .bind(settled.kind.as_str())
            .bind(key.parse::<i64>().ok())
            .bind(key.parse::<uuid::Uuid>().ok())
            .bind(key)
            .fetch_all(self.pool())
            .await?;
        match (found.len(), project_id) {
            (0, Some(_)) => Err(invalid(REFERENCE_NOT_IN_PROJECT)),
            (0, None) => Err(invalid("Knowledge reference not found")),
            (1, _) => Ok(with_correction(found.remove(0), settled.corrected)),
            _ => Err(invalid(
                "Knowledge reference matches several projects; pass project",
            )),
        }
    }
}
