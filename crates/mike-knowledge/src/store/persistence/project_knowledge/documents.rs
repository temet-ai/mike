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
        let all = SourceColumns {
            owners: inputs.iter().map(|input| input.location_uuid).collect(),
            names: inputs.iter().map(|input| input.name.clone()).collect(),
            roots: inputs.iter().map(|input| input.root_path.clone()).collect(),
        };
        let mut registered = self.upsert_location_sources(&all).await?;
        let moved = unmatched(&registered);
        if moved.is_empty() {
            return Ok(registered);
        }
        let retry = all.pick(&moved);
        sqlx::query!(
            "UPDATE memory.knowledge_sources s SET root_path = x.root_path
               FROM unnest($1::uuid[], $2::text[], $3::text[]) AS x(location_uuid, name, root_path)
              WHERE s.location_uuid = x.location_uuid AND s.name = x.name
                AND s.root_path <> x.root_path",
            &retry.owners,
            &retry.names,
            &retry.roots
        )
        .execute(self.pool())
        .await?;
        let again = self.upsert_location_sources(&retry).await?;
        refill(&mut registered, &moved, again);
        Ok(registered)
    }

    async fn upsert_location_sources(
        self,
        columns: &SourceColumns,
    ) -> Result<Vec<Option<RegisteredSource>>, PersistError> {
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
            &columns.owners,
            &columns.names,
            &columns.roots
        )
        .fetch_all(self.pool())
        .await?;
        slots(
            columns.names.len(),
            rows.into_iter()
                .map(|row| (row.ord, row.source_uuid, row.generation)),
        )
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
        let all = SourceColumns {
            owners: inputs.iter().map(|input| input.machine_uuid).collect(),
            names: inputs.iter().map(|input| input.name.clone()).collect(),
            roots: inputs.iter().map(|input| input.root_path.clone()).collect(),
        };
        let mut registered = self.upsert_machine_sources(&all).await?;
        let moved = unmatched(&registered);
        if moved.is_empty() {
            return Ok(registered);
        }
        let retry = all.pick(&moved);
        sqlx::query!(
            "UPDATE memory.knowledge_sources s SET root_path = x.root_path
               FROM unnest($1::uuid[], $2::text[], $3::text[]) AS x(machine_uuid, name, root_path)
              WHERE s.machine_uuid = x.machine_uuid AND s.name = x.name
                AND s.root_path <> x.root_path",
            &retry.owners,
            &retry.names,
            &retry.roots
        )
        .execute(self.pool())
        .await?;
        let again = self.upsert_machine_sources(&retry).await?;
        refill(&mut registered, &moved, again);
        Ok(registered)
    }

    async fn upsert_machine_sources(
        self,
        columns: &SourceColumns,
    ) -> Result<Vec<Option<RegisteredSource>>, PersistError> {
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
            &columns.owners,
            &columns.names,
            &columns.roots
        )
        .fetch_all(self.pool())
        .await?;
        slots(
            columns.names.len(),
            rows.into_iter()
                .map(|row| (row.ord, row.source_uuid, row.generation)),
        )
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

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    /// Records the fingerprint of the document set an ingest produced at
    /// `generation`, so a later scan of the same set can skip the ingest.
    pub async fn record_scan_fingerprint(
        self,
        source: Uuid,
        generation: i64,
        fingerprint: &str,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "UPDATE memory.knowledge_sources
                SET scan_fingerprint = $3, scan_fingerprint_generation = $2
              WHERE source_uuid = $1 AND generation = $2",
            source,
            generation,
            fingerprint
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Whether the source's last recorded ingest, at `generation`, had this
    /// fingerprint.
    pub async fn scan_fingerprint_matches(
        self,
        source: Uuid,
        generation: i64,
        fingerprint: &str,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT EXISTS (
                 SELECT 1 FROM memory.knowledge_sources
                  WHERE source_uuid = $1
                    AND scan_fingerprint_generation = $2
                    AND scan_fingerprint = $3) AS "matches!""#,
            source,
            generation,
            fingerprint
        )
        .fetch_one(self.pool())
        .await?)
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

/// Source registrations as parallel columns; `owners` holds location or
/// machine uuids. A registration whose stored root differs comes back
/// unmatched, and only those get their root moved and are registered again.
struct SourceColumns {
    owners: Vec<Uuid>,
    names: Vec<String>,
    roots: Vec<String>,
}

impl SourceColumns {
    fn pick(&self, indexes: &[usize]) -> SourceColumns {
        SourceColumns {
            owners: indexes.iter().map(|&i| self.owners[i]).collect(),
            names: indexes.iter().map(|&i| self.names[i].clone()).collect(),
            roots: indexes.iter().map(|&i| self.roots[i].clone()).collect(),
        }
    }
}

fn unmatched(registered: &[Option<RegisteredSource>]) -> Vec<usize> {
    registered
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.is_none())
        .map(|(index, _)| index)
        .collect()
}

fn refill(
    registered: &mut [Option<RegisteredSource>],
    indexes: &[usize],
    again: Vec<Option<RegisteredSource>>,
) {
    for (&index, slot) in indexes.iter().zip(again) {
        registered[index] = slot;
    }
}

fn slots(
    len: usize,
    rows: impl Iterator<Item = (i64, Uuid, i64)>,
) -> Result<Vec<Option<RegisteredSource>>, PersistError> {
    let mut registered: Vec<Option<RegisteredSource>> =
        std::iter::repeat_with(|| None).take(len).collect();
    for (ord, source_uuid, generation) in rows {
        let slot = usize::try_from(ord - 1)
            .ok()
            .and_then(|index| registered.get_mut(index))
            .ok_or_else(|| invalid("Source registration answered an unknown position"))?;
        *slot = Some(RegisteredSource {
            source_uuid,
            generation,
        });
    }
    Ok(registered)
}
