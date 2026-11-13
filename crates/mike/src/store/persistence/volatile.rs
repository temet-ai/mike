use chrono::{DateTime, Utc};

use super::PersistError;

#[derive(Debug, Clone)]
pub(crate) struct VolatilePatternRow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) pattern: String,
    pub(crate) applies_to_block_index: u8,
    pub(crate) enabled: bool,
    pub(crate) notes: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct VolatilePatternDbRow {
    id: i64,
    name: String,
    pattern: String,
    applies_to_block_index: i32,
    enabled: bool,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<VolatilePatternDbRow> for VolatilePatternRow {
    type Error = PersistError;
    fn try_from(row: VolatilePatternDbRow) -> Result<Self, Self::Error> {
        let applies_to_block_index: u8 = match row.applies_to_block_index {
            0..=2 => row.applies_to_block_index as u8,
            other => {
                return Err(PersistError::DataInvariant(format!(
                    "volatile_line_patterns row id={} carries applies_to_block_index={other} \
                     outside of [0,2]; the CHECK constraint should have rejected this row",
                    row.id
                )));
            }
        };
        Ok(VolatilePatternRow {
            id: row.id,
            name: row.name,
            pattern: row.pattern,
            applies_to_block_index,
            enabled: row.enabled,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

pub(crate) struct VolatilePatternPatch<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) pattern: Option<&'a str>,
    pub(crate) applies_to_block_index: Option<u8>,
    pub(crate) enabled: Option<bool>,
    pub(crate) notes: Option<Option<&'a str>>,
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub(crate) async fn list_volatile_patterns(
        self,
    ) -> Result<Vec<VolatilePatternRow>, PersistError> {
        crate::metrics::time_db("list_volatile_patterns", async {
            let rows = sqlx::query_as!(
                VolatilePatternDbRow,
                "SELECT id, name, pattern, applies_to_block_index, enabled, notes, created_at, updated_at
                 FROM rules.volatile_line_patterns
                 ORDER BY created_at ASC",
            )
            .fetch_all(&self.pool)
            .await?;
            rows.into_iter().map(VolatilePatternRow::try_from).collect()
        })
        .await
    }

    pub(crate) async fn fetch_volatile_pattern_by_id(
        self,
        id: i64,
    ) -> Result<Option<VolatilePatternRow>, PersistError> {
        crate::metrics::time_db("fetch_volatile_pattern_by_id", async move {
            let row = sqlx::query_as!(
                VolatilePatternDbRow,
                "SELECT id, name, pattern, applies_to_block_index, enabled, notes, created_at, updated_at
                 FROM rules.volatile_line_patterns
                 WHERE id = $1",
                id,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(r) => Ok(Some(VolatilePatternRow::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub(crate) async fn create_volatile_pattern(
        self,
        name: &str,
        pattern: &str,
        applies_to_block_index: u8,
        enabled: bool,
        notes: Option<&str>,
    ) -> Result<VolatilePatternRow, PersistError> {
        crate::metrics::time_db("create_volatile_pattern", async move {
            let block_index = i32::from(applies_to_block_index);
            let row = sqlx::query_as!(
                VolatilePatternDbRow,
                "INSERT INTO rules.volatile_line_patterns
                    (name, pattern, applies_to_block_index, enabled, notes)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING id, name, pattern, applies_to_block_index, enabled, notes, created_at, updated_at",
                name,
                pattern,
                block_index,
                enabled,
                notes,
            )
            .fetch_one(&self.pool)
            .await?;
            VolatilePatternRow::try_from(row)
        })
        .await
    }

    pub(crate) async fn patch_volatile_pattern(
        self,
        id: i64,
        patch: VolatilePatternPatch<'_>,
    ) -> Result<Option<VolatilePatternRow>, PersistError> {
        crate::metrics::time_db("patch_volatile_pattern", async move {
            let notes_set = patch.notes.is_some();
            let notes_value: Option<&str> = patch.notes.flatten();
            let block_index = patch.applies_to_block_index.map(i32::from);
            let row = sqlx::query_as!(
                VolatilePatternDbRow,
                "UPDATE rules.volatile_line_patterns
                 SET name                   = COALESCE($2, name),
                     pattern                = COALESCE($3, pattern),
                     applies_to_block_index = COALESCE($4, applies_to_block_index),
                     enabled                = COALESCE($5, enabled),
                     notes                  = CASE WHEN $6 THEN $7 ELSE notes END,
                     updated_at             = NOW()
                 WHERE id = $1
                 RETURNING id, name, pattern, applies_to_block_index, enabled, notes, created_at, updated_at",
                id,
                patch.name,
                patch.pattern,
                block_index,
                patch.enabled,
                notes_set,
                notes_value,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(r) => Ok(Some(VolatilePatternRow::try_from(r)?)),
                None => Ok(None),
            }
        })
        .await
    }

    pub(crate) async fn delete_volatile_pattern(self, id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("delete_volatile_pattern", async move {
            let result = sqlx::query!("DELETE FROM rules.volatile_line_patterns WHERE id = $1", id)
                .execute(&self.pool)
                .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub(crate) async fn reload_volatile_cache(
        self,
        cache: &crate::compose_prompt::volatile::VolatilePatternCache,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("reload_volatile_cache", async move {
            let rows = self.list_volatile_patterns().await?;
            let mut compiled = Vec::with_capacity(rows.len());
            for row in &rows {
                match crate::compose_prompt::volatile::compile_row(row) {
                    Ok(p) => {
                        tracing::trace!(
                            id = %p.id,
                            name = %p.name,
                            block = p.applies_to_block_index,
                            enabled = p.enabled,
                            "loaded volatile-line pattern into cache"
                        );
                        compiled.push(p);
                    }
                    Err(err) => {
                        tracing::warn!(
                            id = %row.id,
                            name = %row.name,
                            error = %err,
                            "skipping volatile-line pattern with invalid regex during cache reload"
                        );
                    }
                }
            }
            cache.store(compiled);
            Ok(())
        })
        .await
    }
}
