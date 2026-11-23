use super::answer::{JudgeAnswer, MissingAnswer, SplitAnswer};
use super::llm::{ConsolidationLlm, Embedder, Failure};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{
    DocumentInput, KnowledgeEmbeddingSource, LocationInput, ScanInput, SourceInput,
};
use crate::store::persistence::test_support::{TestDb, TestDbHandle, seed_project};
use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use uuid::Uuid;

fn hex_of(body: &str) -> String {
    Sha256::digest(body.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub struct Fixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct Carrier {
    pub item_id: i64,
    pub source_id: i64,
    pub reference: String,
}

pub async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture {
        db,
        handle,
        model: format!("test-model-{}", Uuid::new_v4().simple()),
    }))
}

pub fn vector(spikes: &[(usize, f32)]) -> String {
    let mut values = vec![0.0_f32; 768];
    for (axis, weight) in spikes {
        values[*axis] = *weight;
    }
    let parts: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    format!("[{}]", parts.join(","))
}

pub struct FileCarrier {
    pub carrier: Carrier,
    pub machine: Uuid,
    pub source_uuid: Uuid,
}

impl Fixture {
    pub async fn project(&self) -> anyhow::Result<i64> {
        let id = seed_project(
            self.handle.pool(),
            &format!("/test/knowledge-consolidation-{}", Uuid::new_v4()),
        )
        .await?;
        Ok(id)
    }

    async fn project_path(&self, project_id: i64) -> anyhow::Result<String> {
        Ok(
            sqlx::query_scalar("SELECT project_path FROM projects.projects WHERE id = $1")
                .bind(project_id)
                .fetch_one(self.handle.pool())
                .await?,
        )
    }

    pub async fn lesson(&self, project_id: i64, text: &str) -> anyhow::Result<Carrier> {
        let lesson_id: i64 = sqlx::query_scalar(
            "INSERT INTO memory.lessons (project_id, kind_id, text)
             SELECT $1, k.id, $2 FROM memory.lesson_kinds k ORDER BY k.id LIMIT 1
             RETURNING id",
        )
        .bind(project_id)
        .bind(text)
        .fetch_one(self.handle.pool())
        .await?;
        let carrier = self
            .carrier("lesson", lesson_id, &format!("lesson:{lesson_id}"))
            .await?;
        self.embed(&carrier, text).await?;
        Ok(carrier)
    }

    pub async fn evidence(
        &self,
        kind: &str,
        project_id: i64,
        body: &str,
        embedding: &str,
    ) -> anyhow::Result<Carrier> {
        let source_id = i64::from(Uuid::new_v4().as_u128() as u32) + 1_000_000_000;
        sqlx::query(
            "INSERT INTO memory.knowledge_items (kind, source_id, project_id, needs_embedding)
             VALUES ($1, $2, $3, false)",
        )
        .bind(kind)
        .bind(source_id)
        .bind(project_id)
        .execute(self.handle.pool())
        .await?;
        let carrier = self.carrier(kind, source_id, &format!("{kind}:{source_id}")).await?;
        self.embed_with(&carrier, body, embedding).await?;
        Ok(carrier)
    }

    async fn carrier(&self, kind: &str, source_id: i64, reference: &str) -> anyhow::Result<Carrier> {
        let item_id: i64 = sqlx::query_scalar(
            "SELECT id FROM memory.knowledge_items WHERE kind = $1 AND source_id = $2",
        )
        .bind(kind)
        .bind(source_id)
        .fetch_one(self.handle.pool())
        .await?;
        Ok(Carrier {
            item_id,
            source_id,
            reference: reference.into(),
        })
    }

    pub async fn embed(&self, carrier: &Carrier, body: &str) -> anyhow::Result<()> {
        self.embed_with(carrier, body, &vector(&[(0, 1.0)])).await
    }

    pub async fn embed_with(
        &self,
        carrier: &Carrier,
        body: &str,
        embedding: &str,
    ) -> anyhow::Result<()> {
        let (kind, project_id, changed_at): (String, Option<i64>, DateTime<Utc>) = sqlx::query_as(
            "SELECT kind, project_id, changed_at FROM memory.knowledge_items WHERE id = $1",
        )
        .bind(carrier.item_id)
        .fetch_one(self.handle.pool())
        .await?;
        let hash = hex_of(body);
        let source = KnowledgeEmbeddingSource {
            item_id: carrier.item_id,
            changed_at,
            reference: carrier.reference.clone(),
            source_kind: kind.clone(),
            source_id: carrier.source_id,
            project_id,
            label: kind,
            name: "name".into(),
            header: "header".into(),
            body: body.into(),
        };
        let records = json!([{"ordinal": 0, "body": body, "embedding": embedding}]);
        self.handle
            .knowledge()
            .save_knowledge_embeddings(&source, &self.model, &hash, records)
            .await?;
        Ok(())
    }

    pub async fn checkout_file(
        &self,
        project_id: i64,
        source_name: &str,
        relative_path: &str,
        body: &str,
        root: &std::path::Path,
    ) -> anyhow::Result<FileCarrier> {
        let machine = Uuid::new_v4();
        let location = self
            .handle
            .knowledge()
            .register_project_location(&LocationInput {
                project: self.project_path(project_id).await?,
                machine_uuid: machine,
                machine_name: format!("host-{machine}"),
                root_path: format!("/checkout/{machine}"),
            })
            .await?;
        let source = self
            .handle
            .knowledge()
            .register_knowledge_source(&SourceInput {
                location_uuid: serde_json::from_value(location["location_uuid"].clone())?,
                name: source_name.into(),
                root_path: root.to_string_lossy().into_owned(),
            })
            .await?;
        let source_uuid: Uuid = serde_json::from_value(source["source_uuid"].clone())?;
        let generation: i64 = sqlx::query_scalar(
            "SELECT generation FROM memory.knowledge_sources WHERE source_uuid = $1",
        )
        .bind(source_uuid)
        .fetch_one(self.handle.pool())
        .await?;
        let document = DocumentInput {
            relative_path: relative_path.into(),
            title: "Guidance".into(),
            content_hash: hex_of(body),
            body: body.into(),
            kind: "rule".into(),
            status: "current".into(),
            metadata: json!({}),
        };
        self.handle
            .knowledge()
            .ingest_knowledge_scan(
                source_uuid,
                &ScanInput {
                    generation,
                    documents: vec![document],
                },
            )
            .await?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT d.current_revision FROM memory.knowledge_documents d
              WHERE d.source_uuid = $1 AND d.relative_path = $2",
        )
        .bind(source_uuid)
        .bind(relative_path)
        .fetch_one(self.handle.pool())
        .await?;
        let carrier = self
            .carrier("artifact", revision, &format!("artifact:{revision}"))
            .await?;
        self.embed(&carrier, body).await?;
        Ok(FileCarrier {
            carrier,
            machine,
            source_uuid,
        })
    }

    pub async fn revise_file(&self, file: &FileCarrier, relative_path: &str, body: &str) -> anyhow::Result<Carrier> {
        let generation: i64 = sqlx::query_scalar("SELECT generation FROM memory.knowledge_sources WHERE source_uuid = $1")
            .bind(file.source_uuid)
            .fetch_one(self.handle.pool())
            .await?;
        let document = DocumentInput {
            relative_path: relative_path.into(),
            title: "Guidance".into(),
            content_hash: hex_of(body),
            body: body.into(),
            kind: "rule".into(),
            status: "current".into(),
            metadata: json!({}),
        };
        self.handle
            .knowledge()
            .ingest_knowledge_scan(file.source_uuid, &ScanInput { generation, documents: vec![document] })
            .await?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT current_revision FROM memory.knowledge_documents WHERE source_uuid = $1 AND relative_path = $2",
        )
        .bind(file.source_uuid)
        .bind(relative_path)
        .fetch_one(self.handle.pool())
        .await?;
        let carrier = self.carrier("artifact", revision, &format!("artifact:{revision}")).await?;
        self.embed(&carrier, body).await?;
        Ok(carrier)
    }

    pub async fn handle_requeue(&self, carrier: &Carrier) -> anyhow::Result<()> {
        self.handle.knowledge().queue_knowledge_consolidation(&[carrier.item_id]).await?;
        Ok(())
    }

    pub async fn consolidate_at(&self, item_id: i64) -> anyhow::Result<Option<DateTime<Utc>>> {
        Ok(
            sqlx::query_scalar("SELECT consolidate_at FROM memory.knowledge_items WHERE id = $1")
                .bind(item_id)
                .fetch_one(self.handle.pool())
                .await?,
        )
    }

    pub async fn statements_of(&self, item_id: i64) -> anyhow::Result<Vec<(i64, String, String, Option<i64>)>> {
        Ok(sqlx::query_as(
            "SELECT id, text, state, subject_id FROM memory.knowledge_statements
              WHERE item_id = $1 ORDER BY ordinal",
        )
        .bind(item_id)
        .fetch_all(self.handle.pool())
        .await?)
    }

    pub async fn make_subjects_due(&self) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE memory.knowledge_subjects SET judge_after = now() - interval '1 second'
              WHERE changed_at IS NOT NULL",
        )
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub async fn retirement(&self, lesson_id: i64) -> anyhow::Result<(Option<String>, Option<i64>, String)> {
        Ok(sqlx::query_as(
            "SELECT retired_kind, retired_by_lesson_id, text FROM memory.lessons WHERE id = $1",
        )
        .bind(lesson_id)
        .fetch_one(self.handle.pool())
        .await?)
    }
}

pub struct StubLlm {
    pub splits: Vec<Mutex<VecDeque<Result<SplitAnswer, Failure>>>>,
    pub judges: Mutex<VecDeque<Result<JudgeAnswer, Failure>>>,
    pub checks: Mutex<VecDeque<Result<MissingAnswer, Failure>>>,
    pub split_calls: Mutex<Vec<usize>>,
    pub split_bodies: Mutex<Vec<String>>,
    pub judge_calls: Mutex<Vec<String>>,
}

impl StubLlm {
    pub fn new(models: usize) -> Self {
        Self {
            splits: (0..models).map(|_| Mutex::new(VecDeque::new())).collect(),
            judges: Mutex::new(VecDeque::new()),
            checks: Mutex::new(VecDeque::new()),
            split_calls: Mutex::new(Vec::new()),
            split_bodies: Mutex::new(Vec::new()),
            judge_calls: Mutex::new(Vec::new()),
        }
    }

    pub fn push_split(&self, model: usize, answer: Result<SplitAnswer, Failure>) {
        if let Some(queue) = self.splits.get(model) {
            queue.lock().unwrap_or_else(|e| e.into_inner()).push_back(answer);
        }
    }

    pub fn push_judge(&self, answer: Result<JudgeAnswer, Failure>) {
        self.judges.lock().unwrap_or_else(|e| e.into_inner()).push_back(answer);
    }

    pub fn push_check(&self, answer: Result<MissingAnswer, Failure>) {
        self.checks.lock().unwrap_or_else(|e| e.into_inner()).push_back(answer);
    }

    pub fn judged(&self) -> usize {
        self.judge_calls.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

fn pop<T>(queue: &Mutex<VecDeque<Result<T, Failure>>>) -> Result<T, Failure> {
    queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .pop_front()
        .unwrap_or_else(|| Err(Failure::permanent("the stub has no answer queued")))
}

impl ConsolidationLlm for StubLlm {
    fn split_models(&self) -> usize {
        self.splits.len()
    }

    async fn split(&self, index: usize, body: &str) -> Result<(SplitAnswer, String), Failure> {
        self.split_calls.lock().unwrap_or_else(|e| e.into_inner()).push(index);
        self.split_bodies.lock().unwrap_or_else(|e| e.into_inner()).push(body.to_owned());
        let queue = self
            .splits
            .get(index)
            .ok_or_else(|| Failure::permanent("no such split model"))?;
        pop(queue).map(|answer| (answer, format!("split-{index}")))
    }

    async fn judge(&self, user: &str) -> Result<(JudgeAnswer, String), Failure> {
        self.judge_calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(user.to_owned());
        pop(&self.judges).map(|answer| (answer, "stub-judge".to_owned()))
    }

    async fn check_missing(&self, _user: &str) -> Result<(MissingAnswer, String), Failure> {
        pop(&self.checks).map(|answer| (answer, "stub-check".to_owned()))
    }
}

pub struct StubEmbedder {
    pub available: bool,
    pub vectors: Mutex<HashMap<String, String>>,
    pub failure: Mutex<Option<Failure>>,
}

impl Default for StubEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl StubEmbedder {
    pub fn new() -> Self {
        Self {
            available: true,
            vectors: Mutex::new(HashMap::new()),
            failure: Mutex::new(None),
        }
    }

    pub fn set(&self, text: &str, spikes: &[(usize, f32)]) {
        self.vectors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(text.to_owned(), vector(spikes));
    }

    pub fn fail_with(&self, failure: Option<Failure>) {
        *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = failure;
    }
}

impl Embedder for StubEmbedder {
    fn available(&self) -> bool {
        self.available
    }

    fn model_key(&self) -> String {
        "stub-embedder".to_owned()
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<String>, Failure> {
        if let Some(failure) = self.failure.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Err(failure);
        }
        let vectors = self.vectors.lock().unwrap_or_else(|e| e.into_inner());
        Ok(texts
            .iter()
            .map(|text| {
                vectors.get(text).cloned().unwrap_or_else(|| {
                    let axis = 100 + text.bytes().map(usize::from).sum::<usize>() % 600;
                    vector(&[(axis, 1.0)])
                })
            })
            .collect())
    }
}

pub use super::harness::*;
