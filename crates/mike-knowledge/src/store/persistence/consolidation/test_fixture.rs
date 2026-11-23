use crate::store::persistence::PersistHandle;
use crate::store::persistence::consolidation::{NewStatement, NewStatements};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::{
    DocumentInput, KnowledgeEmbeddingSource, LocationInput, ScanInput, SourceInput,
};
use crate::store::persistence::test_support::{TestDb, TestDbHandle, seed_project};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub struct Fixture {
    pub db: TestDbHandle,
    pub handle: PersistHandle,
    pub model: String,
}

pub struct Carrier {
    pub item_id: i64,
    pub source_id: i64,
    pub reference: String,
}

pub async fn fixture(isolated: bool) -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = if isolated {
        test_db.acquire_isolated().await?
    } else {
        test_db.acquire_shared().await?
    };
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

pub fn statement(text: &str, embedding: &str) -> NewStatement {
    NewStatement {
        text: text.into(),
        passage_start: None,
        passage_end: None,
        embedding: embedding.into(),
    }
}

pub fn statements_of(hash: &str, items: Vec<NewStatement>) -> NewStatements {
    NewStatements {
        split_hash: hash.into(),
        model_key: "test-model".into(),
        stated_at: Utc::now(),
        judge_after: Utc::now() - Duration::seconds(1),
        statements: items,
    }
}

impl Fixture {
    pub async fn project(&self) -> anyhow::Result<i64> {
        seed_project(
            self.handle.pool(),
            &format!("/test/consolidation-{}", Uuid::new_v4()),
        )
        .await
    }

    pub async fn project_path(&self, project_id: i64) -> anyhow::Result<String> {
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
        self.carrier("lesson", lesson_id, &format!("lesson:{}", Uuid::new_v4()))
            .await
    }

    pub async fn evidence_item(
        &self,
        kind: &str,
        project_id: i64,
        reference: &str,
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
        self.carrier(kind, source_id, reference).await
    }

    pub async fn carrier(
        &self,
        kind: &str,
        source_id: i64,
        reference: &str,
    ) -> anyhow::Result<Carrier> {
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

    pub async fn embed(&self, carrier: &Carrier, hash: &str, chunk: &str) -> anyhow::Result<()> {
        let (kind, project_id, changed_at): (String, Option<i64>, DateTime<Utc>) = sqlx::query_as(
            "SELECT kind, project_id, changed_at FROM memory.knowledge_items WHERE id = $1",
        )
        .bind(carrier.item_id)
        .fetch_one(self.handle.pool())
        .await?;
        let records = json!([{"ordinal": 0, "body": chunk, "embedding": vector(&[(0, 1.0)])}]);
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
            body: chunk.into(),
        };
        self.handle
            .knowledge()
            .save_knowledge_embeddings(&source, &self.model, hash, records)
            .await?;
        Ok(())
    }

    pub async fn chunk(&self, carrier: &Carrier, body: &str, embedding: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO memory.knowledge_embedding_state
                 (item_id, source_kind, source_id, reference, project_id, model_key, completed_at)
             SELECT i.id, i.kind, i.source_id, $2, i.project_id, $3, now()
               FROM memory.knowledge_items i WHERE i.id = $1
             ON CONFLICT (item_id) DO NOTHING",
        )
        .bind(carrier.item_id)
        .bind(&carrier.reference)
        .bind(&self.model)
        .execute(self.handle.pool())
        .await?;
        sqlx::query(
            "INSERT INTO memory.knowledge_embedding_chunks (state_id, ordinal, body, embedding)
             SELECT e.id, 0, $2, $3::public.vector
               FROM memory.knowledge_embedding_state e WHERE e.item_id = $1",
        )
        .bind(carrier.item_id)
        .bind(body)
        .bind(embedding)
        .execute(self.handle.pool())
        .await?;
        Ok(())
    }

    pub async fn checkout_file(
        &self,
        project_id: i64,
        source_name: &str,
        relative_path: &str,
        body: &str,
    ) -> anyhow::Result<Carrier> {
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
                root_path: format!("/roots/{machine}"),
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
            content_hash: Sha256::digest(body.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
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
        self.carrier("artifact", revision, &format!("artifact:{revision}"))
            .await
    }

    pub async fn revise_file(&self, carrier: &Carrier, body: &str) -> anyhow::Result<Carrier> {
        let (source_uuid, relative_path, generation): (Uuid, String, i64) = sqlx::query_as(
            "SELECT d.source_uuid, d.relative_path, s.generation
               FROM memory.knowledge_revisions r
               JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
               JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
              WHERE r.id = $1",
        )
        .bind(carrier.source_id)
        .fetch_one(self.handle.pool())
        .await?;
        let document = DocumentInput {
            relative_path: relative_path.clone(),
            title: "Guidance".into(),
            content_hash: Sha256::digest(body.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
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
            "SELECT current_revision FROM memory.knowledge_documents WHERE source_uuid = $1 AND relative_path = $2",
        )
        .bind(source_uuid)
        .bind(&relative_path)
        .fetch_one(self.handle.pool())
        .await?;
        self.carrier("artifact", revision, &format!("artifact:{revision}"))
            .await
    }

    pub async fn document_of(&self, carrier: &Carrier) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar("SELECT document_uuid FROM memory.knowledge_revisions WHERE id = $1")
            .bind(carrier.source_id)
            .fetch_one(self.handle.pool())
            .await?)
    }

    pub async fn run(&self) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id)
             SELECT k.id, t.id, s.id
               FROM memory.consolidation_run_kinds k, memory.consolidation_triggers t,
                    memory.consolidation_run_statuses s
              WHERE k.code = 'knowledge' AND t.code = 'manual' AND s.code = 'running'
             RETURNING id",
        )
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub async fn statement_ids(
        &self,
        item_id: i64,
        hash: &str,
        items: Vec<NewStatement>,
    ) -> anyhow::Result<Vec<i64>> {
        let replaced = self
            .handle
            .knowledge()
            .replace_item_statements(item_id, &statements_of(hash, items))
            .await?;
        Ok(replaced.created.into_iter().map(|c| c.id).collect())
    }

    pub async fn consolidate_at(&self, item_id: i64) -> anyhow::Result<Option<DateTime<Utc>>> {
        Ok(
            sqlx::query_scalar("SELECT consolidate_at FROM memory.knowledge_items WHERE id = $1")
                .bind(item_id)
                .fetch_one(self.handle.pool())
                .await?,
        )
    }
}
