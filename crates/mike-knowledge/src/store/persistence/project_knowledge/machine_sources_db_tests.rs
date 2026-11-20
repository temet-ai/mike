use super::tests::document;
use super::{
    DocumentInput, KnowledgeEmbeddingSource, KnowledgeQuery, LocationInput, MachineSourceInput,
    ScanInput, SourceInput,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::test_support::{TestDb, TestDbHandle, seed_project};
use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    _db: TestDbHandle,
    handle: PersistHandle,
    word: String,
    model: String,
}

struct Project {
    path: String,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Fixture {
        _db: db,
        handle,
        word: format!("heron{}", Uuid::new_v4().simple()),
        model: format!("test-model-{}", Uuid::new_v4().simple()),
    }))
}

impl Fixture {
    async fn project(&self) -> anyhow::Result<Project> {
        let path = format!("/test/machine-sources-{}", Uuid::new_v4());
        seed_project(self.handle.pool(), &path).await?;
        Ok(Project { path })
    }

    async fn machine(&self) -> anyhow::Result<Uuid> {
        let machine = Uuid::new_v4();
        sqlx::query("INSERT INTO projects.machines (machine_uuid, name) VALUES ($1, $2)")
            .bind(machine)
            .bind(format!("machine-{machine}"))
            .execute(self.handle.pool())
            .await?;
        Ok(machine)
    }

    async fn location_source(&self, project: &Project, name: &str) -> anyhow::Result<Uuid> {
        let machine = Uuid::new_v4();
        let row = self
            .handle
            .knowledge()
            .register_project_location(&LocationInput {
                project: project.path.clone(),
                machine_uuid: machine,
                machine_name: format!("host-{machine}"),
                root_path: format!("/checkout/{machine}"),
            })
            .await?;
        let registered = self
            .handle
            .knowledge()
            .register_knowledge_source(&SourceInput {
                location_uuid: serde_json::from_value(row["location_uuid"].clone())?,
                name: name.into(),
                root_path: format!("/roots/{name}"),
            })
            .await?;
        Ok(serde_json::from_value(registered["source_uuid"].clone())?)
    }

    async fn machine_source(&self, machine: Uuid, name: &str) -> anyhow::Result<Uuid> {
        let registered = self
            .handle
            .knowledge()
            .register_machine_knowledge_sources(&[MachineSourceInput {
                machine_uuid: machine,
                name: name.into(),
                root_path: format!("/home/{machine}/.claude/{name}"),
            }])
            .await?;
        let source = registered
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| anyhow::anyhow!("machine source was not registered"))?;
        Ok(source.source_uuid)
    }

    async fn scan(&self, source: Uuid, documents: Vec<DocumentInput>) -> anyhow::Result<()> {
        let generation: i64 = sqlx::query_scalar(
            "SELECT generation FROM memory.knowledge_sources WHERE source_uuid = $1",
        )
        .bind(source)
        .fetch_one(self.handle.pool())
        .await?;
        self.handle
            .knowledge()
            .ingest_knowledge_scan(
                source,
                &ScanInput {
                    generation,
                    documents,
                },
            )
            .await?;
        Ok(())
    }

    async fn revision(&self, source: Uuid, path: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT d.current_revision FROM memory.knowledge_documents d
              WHERE d.source_uuid = $1 AND d.relative_path = $2",
        )
        .bind(source)
        .bind(path)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn item(&self, revision: i64) -> anyhow::Result<(i64, Option<i64>, bool)> {
        Ok(sqlx::query_as(
            "SELECT id, project_id, needs_embedding FROM memory.knowledge_items
              WHERE kind = 'artifact' AND source_id = $1",
        )
        .bind(revision)
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn found(&self, project: &Project, revision: i64) -> anyhow::Result<bool> {
        let found = self
            .handle
            .knowledge()
            .search_project_knowledge(&KnowledgeQuery {
                project: project.path.clone(),
                query: self.word.clone(),
                ..KnowledgeQuery::default()
            })
            .await?;
        let wanted = format!("artifact:{revision}");
        Ok(found["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["reference"] == json!(wanted))))
    }

    async fn embed(&self, revision: i64, chunks: usize) -> anyhow::Result<()> {
        let (item_id, project_id, _) = self.item(revision).await?;
        let changed_at: DateTime<Utc> =
            sqlx::query_scalar("SELECT changed_at FROM memory.knowledge_items WHERE id = $1")
                .bind(item_id)
                .fetch_one(self.handle.pool())
                .await?;
        let vector = format!("[{}]", vec!["0.01"; 768].join(","));
        let records: Vec<_> = (0..chunks)
            .map(|ordinal| json!({"ordinal": ordinal, "body": format!("chunk {ordinal}"), "embedding": vector}))
            .collect();
        let source = KnowledgeEmbeddingSource {
            item_id,
            changed_at,
            reference: format!("artifact:{revision}"),
            source_kind: "artifact".into(),
            source_id: revision,
            project_id,
            label: "artifact".into(),
            name: "design".into(),
            header: "artifact: design".into(),
            body: "body".into(),
        };
        self.handle
            .knowledge()
            .save_knowledge_embeddings(&source, &self.model, "sha256:test", json!(records))
            .await?;
        Ok(())
    }

    async fn share(&self, revision: i64) -> anyhow::Result<Vec<i64>> {
        let (item_id, _, _) = self.item(revision).await?;
        let served = self
            .handle
            .knowledge()
            .share_twin_knowledge_embeddings(&[item_id], &self.model)
            .await?;
        Ok(served)
    }

    async fn chunk_count(&self, revision: i64) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM memory.knowledge_embedding_chunks c
               JOIN memory.knowledge_embedding_state e ON e.id = c.state_id
              WHERE e.source_kind = 'artifact' AND e.source_id = $1 AND e.model_key = $2",
        )
        .bind(revision)
        .bind(&self.model)
        .fetch_one(self.handle.pool())
        .await?)
    }

    fn doc(&self, path: &str, tail: &str) -> DocumentInput {
        DocumentInput {
            relative_path: path.into(),
            ..document(&format!("{} {tail}", self.word), "current")
        }
    }
}

#[tokio::test]
async fn machine_source_document_is_found_for_every_project() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let (first, second) = (fx.project().await?, fx.project().await?);
    let machine = fx.machine().await?;
    let source = fx
        .machine_source(machine, &format!("rules-{}", Uuid::new_v4()))
        .await?;
    fx.scan(source, vec![fx.doc("shared.md", "global guidance")])
        .await?;
    let revision = fx.revision(source, "shared.md").await?;

    assert!(fx.found(&first, revision).await?);
    assert!(fx.found(&second, revision).await?);
    assert_eq!(fx.item(revision).await?.1, None);
    Ok(())
}

#[tokio::test]
async fn location_source_document_is_found_only_for_its_project() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let (owner, other) = (fx.project().await?, fx.project().await?);
    let source = fx
        .location_source(&owner, &format!("docs-{}", Uuid::new_v4()))
        .await?;
    fx.scan(source, vec![fx.doc("local.md", "project guidance")])
        .await?;
    let revision = fx.revision(source, "local.md").await?;

    assert!(fx.found(&owner, revision).await?);
    assert!(!fx.found(&other, revision).await?);
    Ok(())
}

#[tokio::test]
async fn machine_source_registration_reuses_the_row_for_the_same_name() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let machine = fx.machine().await?;
    let name = format!("agents-{}", Uuid::new_v4());

    let first = fx.machine_source(machine, &name).await?;
    let second = fx.machine_source(machine, &name).await?;

    assert_eq!(first, second);
    Ok(())
}

#[tokio::test]
async fn machine_source_registration_with_a_moved_root_keeps_the_row() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let machine = fx.machine().await?;
    let name = format!("skills-{}", Uuid::new_v4());
    let first = fx.machine_source(machine, &name).await?;

    let moved = fx
        .handle
        .knowledge()
        .register_machine_knowledge_sources(&[MachineSourceInput {
            machine_uuid: machine,
            name: name.clone(),
            root_path: format!("/root/.claude/{name}"),
        }])
        .await?
        .into_iter()
        .next()
        .flatten()
        .ok_or_else(|| anyhow::anyhow!("the moved machine source was not registered"))?;

    assert_eq!(moved.source_uuid, first);
    let root: String =
        sqlx::query_scalar("SELECT root_path FROM memory.knowledge_sources WHERE source_uuid = $1")
            .bind(first)
            .fetch_one(fx.handle.pool())
            .await?;
    assert_eq!(root, format!("/root/.claude/{name}"));
    Ok(())
}

#[tokio::test]
async fn machine_source_missing_root_is_recorded_as_a_failure() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let machine = fx.machine().await?;
    let name = format!("agent-memory-{}", Uuid::new_v4());
    let source = fx.machine_source(machine, &name).await?;
    let input = MachineSourceInput {
        machine_uuid: machine,
        name,
        root_path: "/gone".into(),
    };

    let recorded = fx
        .handle
        .knowledge()
        .record_machine_knowledge_roots_missing(std::slice::from_ref(&input))
        .await?;
    let failure: Option<String> = sqlx::query_scalar(
        "SELECT last_failure FROM memory.knowledge_sources WHERE source_uuid = $1",
    )
    .bind(source)
    .fetch_one(fx.handle.pool())
    .await?;

    assert_eq!(recorded, 1);
    assert!(failure.is_some_and(|text| text.contains("/gone")));
    Ok(())
}

#[tokio::test]
async fn twin_document_is_served_from_the_embedded_twin_without_an_embedder() -> anyhow::Result<()>
{
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let name = format!("skills-{}", Uuid::new_v4());
    let (first, second) = (fx.machine().await?, fx.machine().await?);
    let (one, two) = (
        fx.machine_source(first, &name).await?,
        fx.machine_source(second, &name).await?,
    );
    fx.scan(one, vec![fx.doc("a/SKILL.md", "same body")])
        .await?;
    fx.scan(two, vec![fx.doc("a/SKILL.md", "same body")])
        .await?;
    let (original, copy) = (
        fx.revision(one, "a/SKILL.md").await?,
        fx.revision(two, "a/SKILL.md").await?,
    );
    fx.embed(original, 3).await?;
    let (copy_item, _, waiting) = fx.item(copy).await?;
    assert!(waiting);

    let served = fx.share(copy).await?;

    assert_eq!(served, vec![copy_item]);
    assert_eq!(fx.chunk_count(copy).await?, 3);
    assert_eq!(fx.chunk_count(original).await?, 3);
    assert!(!fx.item(copy).await?.2);
    Ok(())
}

#[tokio::test]
async fn twin_with_a_different_body_is_not_served() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let name = format!("skills-{}", Uuid::new_v4());
    let (first, second) = (fx.machine().await?, fx.machine().await?);
    let (one, two) = (
        fx.machine_source(first, &name).await?,
        fx.machine_source(second, &name).await?,
    );
    fx.scan(one, vec![fx.doc("a/SKILL.md", "old body")]).await?;
    fx.scan(two, vec![fx.doc("a/SKILL.md", "new body")]).await?;
    let (original, other) = (
        fx.revision(one, "a/SKILL.md").await?,
        fx.revision(two, "a/SKILL.md").await?,
    );
    fx.embed(original, 2).await?;

    let served = fx.share(other).await?;

    assert!(served.is_empty());
    assert_eq!(fx.chunk_count(other).await?, 0);
    assert!(fx.item(other).await?.2);
    Ok(())
}

#[tokio::test]
async fn twin_in_another_project_is_not_served() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let name = format!("docs-{}", Uuid::new_v4());
    let (first, second) = (fx.project().await?, fx.project().await?);
    let (one, two) = (
        fx.location_source(&first, &name).await?,
        fx.location_source(&second, &name).await?,
    );
    fx.scan(one, vec![fx.doc("a.md", "same body")]).await?;
    fx.scan(two, vec![fx.doc("a.md", "same body")]).await?;
    let (original, other) = (
        fx.revision(one, "a.md").await?,
        fx.revision(two, "a.md").await?,
    );
    fx.embed(original, 2).await?;

    let served = fx.share(other).await?;

    assert!(served.is_empty());
    assert_eq!(fx.chunk_count(other).await?, 0);
    Ok(())
}

#[tokio::test]
async fn a_merged_machines_source_is_adopted_by_the_machine_that_scans_its_root()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let name = format!("skills-{}", Uuid::new_v4());
    let root = format!("/home/shared-{}/.claude/skills", Uuid::new_v4());
    let (legacy, current) = (fx.machine().await?, fx.machine().await?);
    let register = |machine: Uuid| MachineSourceInput {
        machine_uuid: machine,
        name: name.clone(),
        root_path: root.clone(),
    };
    let first = fx
        .handle
        .knowledge()
        .register_machine_knowledge_sources(&[register(legacy)])
        .await?;
    sqlx::query("DELETE FROM projects.machines WHERE machine_uuid = $1")
        .bind(legacy)
        .execute(fx.handle.pool())
        .await?;
    let second = fx
        .handle
        .knowledge()
        .register_machine_knowledge_sources(&[register(current)])
        .await?;
    let uuid = |found: &[Option<super::RegisteredSource>]| {
        found.first().cloned().flatten().map(|s| s.source_uuid)
    };
    assert!(uuid(&first).is_some());
    assert_eq!(uuid(&first), uuid(&second));
    let owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT machine_uuid FROM memory.knowledge_sources WHERE source_uuid = $1",
    )
    .bind(uuid(&second))
    .fetch_one(fx.handle.pool())
    .await?;
    assert_eq!(owner, Some(current));
    Ok(())
}
