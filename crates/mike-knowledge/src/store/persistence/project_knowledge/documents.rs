use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::invalid;
use crate::store::persistence::PersistError;

#[derive(Deserialize)]
pub struct SourceInput {
    pub location_uuid: Uuid,
    pub name: String,
    pub root_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSourceInput {
    pub machine_uuid: Uuid,
    pub name: String,
    pub root_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredSource {
    pub source_uuid: Uuid,
    pub generation: i64,
}

#[derive(Deserialize, Serialize)]
pub struct DocumentInput {
    pub relative_path: String,
    pub title: String,
    pub body: String,
    pub content_hash: String,
    pub kind: String,
    pub status: String,
    pub metadata: Value,
}

#[derive(Deserialize)]
pub struct ScanInput {
    pub generation: i64,
    pub documents: Vec<DocumentInput>,
}

impl ScanInput {
    fn validate(&self) -> Result<(), PersistError> {
        let mut paths = std::collections::HashSet::new();
        for doc in &self.documents {
            let path = std::path::Path::new(&doc.relative_path);
            if doc.relative_path.is_empty()
                || path.is_absolute()
                || path
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                || !paths.insert(&doc.relative_path)
            {
                return Err(invalid(
                    "Scan contains an invalid or duplicate relative path",
                ));
            }
            let hash = Sha256::digest(doc.body.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            if hash != doc.content_hash || !doc.metadata.is_object() {
                return Err(invalid("Document hash or metadata is invalid"));
            }
        }
        Ok(())
    }
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn register_knowledge_source(
        self,
        input: &SourceInput,
    ) -> Result<Value, PersistError> {
        let registered = self
            .register_knowledge_sources(std::slice::from_ref(input))
            .await?
            .pop()
            .flatten()
            .ok_or_else(|| invalid("Source name already refers to a different root"))?;
        Ok(json!({"source_uuid": registered.source_uuid, "generation": registered.generation}))
    }

    pub async fn register_knowledge_sources(
        self,
        inputs: &[SourceInput],
    ) -> Result<Vec<Option<RegisteredSource>>, PersistError> {
        if inputs
            .iter()
            .any(|input| input.name.trim().is_empty() || input.root_path.trim().is_empty())
        {
            return Err(invalid("Source name and root path must not be empty"));
        }
        let locations: Vec<Uuid> = inputs.iter().map(|input| input.location_uuid).collect();
        let names: Vec<String> = inputs.iter().map(|input| input.name.clone()).collect();
        let roots: Vec<String> = inputs.iter().map(|input| input.root_path.clone()).collect();
        sqlx::query!(
            "UPDATE memory.knowledge_sources s SET root_path = x.root_path
               FROM unnest($1::uuid[], $2::text[], $3::text[]) AS x(location_uuid, name, root_path)
              WHERE s.location_uuid = x.location_uuid AND s.name = x.name
                AND s.root_path <> x.root_path",
            &locations,
            &names,
            &roots
        )
        .execute(self.pool())
        .await?;
        let rows = sqlx::query!(
            r#"WITH input AS (
                   SELECT x.ord, x.location_uuid, x.name, x.root_path
                     FROM unnest($1::uuid[], $2::text[], $3::text[])
                          WITH ORDINALITY AS x(location_uuid, name, root_path, ord)
               ), inserted AS (
                   INSERT INTO memory.knowledge_sources(location_uuid, name, root_path)
                   SELECT location_uuid, name, root_path FROM input ORDER BY ord
                   ON CONFLICT (location_uuid, name) DO SELECT
                   RETURNING location_uuid, name, source_uuid, generation, root_path
               )
               SELECT i.ord AS "ord!", n.source_uuid AS "source_uuid!", n.generation AS "generation!"
                 FROM input i
                 JOIN inserted n ON n.location_uuid = i.location_uuid AND n.name = i.name
                                AND n.root_path = i.root_path"#,
            &locations,
            &names,
            &roots
        )
        .fetch_all(self.pool())
        .await?;
        let mut registered: Vec<Option<RegisteredSource>> =
            std::iter::repeat_with(|| None).take(inputs.len()).collect();
        for row in rows {
            let slot = usize::try_from(row.ord - 1)
                .ok()
                .and_then(|index| registered.get_mut(index))
                .ok_or_else(|| invalid("Source registration answered an unknown position"))?;
            *slot = Some(RegisteredSource {
                source_uuid: row.source_uuid,
                generation: row.generation,
            });
        }
        Ok(registered)
    }

    pub async fn register_machine_knowledge_sources(
        self,
        inputs: &[MachineSourceInput],
    ) -> Result<Vec<Option<RegisteredSource>>, PersistError> {
        if inputs
            .iter()
            .any(|input| input.name.trim().is_empty() || input.root_path.trim().is_empty())
        {
            return Err(invalid("Source name and root path must not be empty"));
        }
        let machines: Vec<Uuid> = inputs.iter().map(|input| input.machine_uuid).collect();
        let names: Vec<String> = inputs.iter().map(|input| input.name.clone()).collect();
        let roots: Vec<String> = inputs.iter().map(|input| input.root_path.clone()).collect();
        sqlx::query!(
            "UPDATE memory.knowledge_sources s SET root_path = x.root_path
               FROM unnest($1::uuid[], $2::text[], $3::text[]) AS x(machine_uuid, name, root_path)
              WHERE s.machine_uuid = x.machine_uuid AND s.name = x.name
                AND s.root_path <> x.root_path",
            &machines,
            &names,
            &roots
        )
        .execute(self.pool())
        .await?;
        let rows = sqlx::query!(
            r#"WITH input AS (
                   SELECT x.ord, x.machine_uuid, x.name, x.root_path
                     FROM unnest($1::uuid[], $2::text[], $3::text[])
                          WITH ORDINALITY AS x(machine_uuid, name, root_path, ord)
               ), adopted AS (
                   UPDATE memory.knowledge_sources s
                      SET machine_uuid = i.machine_uuid
                     FROM input i
                    WHERE s.machine_uuid IS NULL AND s.location_uuid IS NULL
                      AND s.name = i.name AND s.root_path = i.root_path
                      AND NOT EXISTS (SELECT 1 FROM memory.knowledge_sources o
                                       WHERE o.machine_uuid = i.machine_uuid AND o.name = i.name)
                   RETURNING s.machine_uuid, s.name, s.source_uuid, s.generation, s.root_path
               ), inserted AS (
                   INSERT INTO memory.knowledge_sources(machine_uuid, name, root_path)
                   SELECT i.machine_uuid, i.name, i.root_path FROM input i
                    WHERE NOT EXISTS (SELECT 1 FROM adopted a
                                       WHERE a.machine_uuid = i.machine_uuid AND a.name = i.name)
                    ORDER BY i.ord
                   ON CONFLICT (machine_uuid, name) WHERE machine_uuid IS NOT NULL DO SELECT
                   RETURNING machine_uuid, name, source_uuid, generation, root_path
               ), registered AS (
                   SELECT * FROM adopted UNION ALL SELECT * FROM inserted
               )
               SELECT i.ord AS "ord!", n.source_uuid AS "source_uuid!", n.generation AS "generation!"
                 FROM input i
                 JOIN registered n ON n.machine_uuid = i.machine_uuid AND n.name = i.name
                                AND n.root_path = i.root_path"#,
            &machines,
            &names,
            &roots
        )
        .fetch_all(self.pool())
        .await?;
        let mut registered: Vec<Option<RegisteredSource>> =
            std::iter::repeat_with(|| None).take(inputs.len()).collect();
        for row in rows {
            let slot = usize::try_from(row.ord - 1)
                .ok()
                .and_then(|index| registered.get_mut(index))
                .ok_or_else(|| invalid("Source registration answered an unknown position"))?;
            *slot = Some(RegisteredSource {
                source_uuid: row.source_uuid,
                generation: row.generation,
            });
        }
        Ok(registered)
    }

    pub async fn ingest_knowledge_scan(
        self,
        source: Uuid,
        input: &ScanInput,
    ) -> Result<Value, PersistError> {
        input.validate()?;
        let mut tx = self.pool().begin().await?;
        let generation: i64 = sqlx::query_scalar(
            "SELECT generation FROM memory.knowledge_sources WHERE source_uuid = $1 FOR UPDATE",
        )
        .bind(source)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| invalid("Unknown knowledge source"))?;
        if generation != input.generation {
            return Err(invalid("Scan generation conflict; rescan before retrying"));
        }
        let docs = serde_json::to_value(&input.documents)?;
        sqlx::query!(
            "INSERT INTO memory.knowledge_documents(source_uuid, relative_path)
             SELECT $1, x.relative_path FROM jsonb_to_recordset($2) AS x(relative_path text)
             ON CONFLICT(source_uuid, relative_path) DO UPDATE SET deleted_at = NULL
             WHERE memory.knowledge_documents.deleted_at IS NOT NULL",
            source,
            &docs
        )
        .execute(&mut *tx)
        .await?;
        let changed = sqlx::query_file!(
            "src/store/persistence/project_knowledge/ingest.sql",
            source,
            &docs
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query!(
            "UPDATE memory.knowledge_documents SET deleted_at = now()
             WHERE source_uuid = $1 AND deleted_at IS NULL AND NOT EXISTS
             (SELECT 1 FROM jsonb_to_recordset($2) AS x(relative_path text)
              WHERE x.relative_path = memory.knowledge_documents.relative_path)",
            source,
            &docs
        )
        .execute(&mut *tx)
        .await?;
        complete_scan(&mut tx, source).await?;
        tx.commit().await?;
        Ok(serde_json::json!({"generation": generation + 1,
            "documents": input.documents.len(), "revisions_written": changed}))
    }
}

async fn complete_scan(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    source: Uuid,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE memory.knowledge_sources SET generation = generation + 1,
            last_scan_at = now() WHERE source_uuid = $1",
        source
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "UPDATE projects.locations SET last_seen = now()
            FROM memory.knowledge_sources s WHERE s.source_uuid = $1
              AND projects.locations.location_uuid = s.location_uuid",
        source
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
