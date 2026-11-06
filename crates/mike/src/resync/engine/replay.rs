use std::collections::HashMap;

use sqlx::{Postgres, QueryBuilder};

use super::row::SourceRow;
use super::{BATCH_SIZE, Class, ResyncEngine, ResyncError, TableSpec};

impl ResyncEngine {
    pub(super) async fn read_cursor(&self, table: &str) -> Result<i64, ResyncError> {
        let last = sqlx::query_scalar!(
            "SELECT last_local_id FROM sync.cursor WHERE table_name = $1",
            table
        )
        .fetch_optional(&self.source)
        .await?;
        Ok(last.unwrap_or(0))
    }

    async fn advance_cursor(&self, table: &str, last_local_id: i64) -> Result<(), ResyncError> {
        sqlx::query!(
            "INSERT INTO sync.cursor (table_name, last_local_id, updated_at)
             VALUES ($1, $2, now())
             ON CONFLICT (table_name) DO UPDATE
                SET last_local_id = EXCLUDED.last_local_id, updated_at = now()",
            table,
            last_local_id,
        )
        .execute(&self.source)
        .await?;
        Ok(())
    }

    async fn read_batch(
        &self,
        spec: &TableSpec,
        cursor: i64,
    ) -> Result<Vec<SourceRow>, ResyncError> {
        let mut select = QueryBuilder::<Postgres>::new("SELECT id");
        for &column in spec.columns {
            select.push(", ").push(column);
            if super::is_vector_column(spec.name, column) {
                select.push("::text AS ").push(column);
            }
        }
        select
            .push(" FROM ")
            .push(spec.name)
            .push(" WHERE id > ")
            .push_bind(cursor)
            .push(" ORDER BY id ASC LIMIT ")
            .push_bind(BATCH_SIZE);
        let rows = select.build().fetch_all(&self.source).await?;
        rows.into_iter()
            .map(|r| SourceRow::parse(spec, r))
            .collect()
    }

    pub(super) async fn replay_table(&self, spec: &TableSpec) -> Result<u64, ResyncError> {
        let mut total: u64 = 0;
        loop {
            let cursor = self.read_cursor(spec.name).await?;
            let rows = self.read_batch(spec, cursor).await?;
            if rows.is_empty() {
                break;
            }
            let max_local_id = rows.iter().map(|r| r.local_id).max().unwrap_or(cursor);

            let inserted = self.replay_batch(spec, rows).await?;
            total += inserted;

            self.advance_cursor(spec.name, max_local_id).await?;
        }
        Ok(total)
    }

    async fn replay_batch(
        &self,
        spec: &TableSpec,
        rows: Vec<SourceRow>,
    ) -> Result<u64, ResyncError> {
        let already: HashMap<i64, i64> = self.load_id_map(spec.name).await?;
        let pending: Vec<SourceRow> = rows
            .into_iter()
            .filter(|r| !already.contains_key(&r.local_id))
            .collect();
        if pending.is_empty() {
            return Ok(0);
        }

        let mut new_mappings: Vec<(i64, i64)> = Vec::new();
        let to_insert: Vec<&SourceRow> = match spec.class {
            Class::ResolveOrInsert => {
                let key = spec.natural_key.ok_or_else(|| {
                    ResyncError::Descriptor(format!(
                        "{} is ResolveOrInsert but declares no natural_key",
                        spec.name
                    ))
                })?;
                let resolved = self.resolve_existing(spec, &key, &pending).await?;
                let mut absent = Vec::new();
                for r in &pending {
                    let source_key = key.normalize(r.column_text(key.column)?);
                    match resolved.get(&super::row::NaturalKey(source_key)) {
                        Some(remote_id) => new_mappings.push((r.local_id, *remote_id)),
                        None => absent.push(r),
                    }
                }
                absent
            }
            Class::InsertAlways => pending.iter().collect(),
        };

        let inserted_count = if to_insert.is_empty() {
            0
        } else {
            let insert_mappings = self.insert_rows(spec, &to_insert).await?;
            new_mappings.extend(insert_mappings);
            to_insert.len() as u64
        };

        if !new_mappings.is_empty() {
            self.record_id_map(spec.name, &new_mappings).await?;
        }

        Ok(inserted_count)
    }
}
