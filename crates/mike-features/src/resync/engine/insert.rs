use std::collections::HashMap;

use sqlx::query_builder::Separated;
use sqlx::{FromRow, Postgres, QueryBuilder};

use super::row::{NaturalKey, SourceRow};
use super::{ResyncEngine, ResyncError, TableSpec, is_vector_column};

type InsertCells<'a> = Separated<'a, Postgres, &'static str>;

#[derive(FromRow)]
struct ResolvedKey {
    k: String,
    id: i64,
}

#[derive(FromRow)]
struct InsertedId {
    id: i64,
}

impl ResyncEngine {
    pub async fn resolve_existing(
        &self,
        spec: &TableSpec,
        key: &super::NaturalKeySpec,
        rows: &[SourceRow],
    ) -> Result<HashMap<NaturalKey, i64>, ResyncError> {
        let keys: Vec<String> = rows
            .iter()
            .map(|r| Ok(key.normalize(r.column_text(key.column)?)))
            .collect::<Result<_, ResyncError>>()?;
        let dest_expr = key.dest_expr;
        let mut select = QueryBuilder::<Postgres>::new("SELECT ");
        select
            .push(dest_expr)
            .push("::text AS k, id FROM ")
            .push(spec.name)
            .push(" WHERE ")
            .push(dest_expr)
            .push("::text = ANY(")
            .push_bind(&keys)
            .push(")");
        let found = select
            .build_query_as::<ResolvedKey>()
            .fetch_all(&self.dest)
            .await?;
        Ok(found
            .into_iter()
            .map(|row| (NaturalKey(row.k), row.id))
            .collect())
    }

    async fn build_fk_maps(
        &self,
        spec: &TableSpec,
    ) -> Result<HashMap<&'static str, HashMap<i64, i64>>, ResyncError> {
        let mut maps: HashMap<&str, HashMap<i64, i64>> = HashMap::new();
        let self_map = self.load_id_map(spec.name).await?;
        for fk in spec.pass1_fks() {
            match fk.references {
                Some(parent) => {
                    if !maps.contains_key(parent) {
                        maps.insert(parent, self.load_id_map(parent).await?);
                    }
                }
                None => {
                    maps.insert(spec.name, self_map.clone());
                }
            }
        }
        Ok(maps)
    }

    fn push_insert_cell(
        spec: &TableSpec,
        r: &SourceRow,
        col: &str,
        maps: &HashMap<&str, HashMap<i64, i64>>,
        cells: &mut InsertCells<'_>,
    ) -> Result<(), ResyncError> {
        let Some(fk) = spec.fks.iter().find(|f| f.column == col) else {
            return r.push_cell(col, cells);
        };
        if fk.deferred {
            cells.push_bind(Option::<i64>::None);
            return Ok(());
        }
        let Some(local_id) = r.fk_value(col) else {
            cells.push_bind(Option::<i64>::None);
            return Ok(());
        };
        let parent = fk.references.unwrap_or(spec.name);
        let remote = maps.get(parent).and_then(|m| m.get(&local_id)).copied();
        let remote = remote.ok_or_else(|| ResyncError::UnmappedFk {
            table: spec.name.to_string(),
            column: col.to_string(),
            references: parent.to_string(),
            local_id,
        })?;
        cells.push_bind(remote);
        Ok(())
    }

    pub async fn insert_rows(
        &self,
        spec: &TableSpec,
        to_insert: &[&SourceRow],
    ) -> Result<Vec<(i64, i64)>, ResyncError> {
        let maps = self.build_fk_maps(spec).await?;

        let mut insert = QueryBuilder::<Postgres>::new("INSERT INTO ");
        insert
            .push(spec.name)
            .push(" (")
            .push(spec.columns.join(", "))
            .push(") VALUES ");
        for (index, r) in to_insert.iter().enumerate() {
            if index > 0 {
                insert.push(", ");
            }
            insert.push("(");
            let mut cells = insert.separated(", ");
            for &col in spec.columns {
                Self::push_insert_cell(spec, r, col, &maps, &mut cells)?;
                if is_vector_column(spec.name, col) {
                    cells.push_unseparated("::vector");
                }
            }
            cells.push_unseparated(")");
        }
        if let Some(target) = spec.on_conflict {
            insert
                .push(" ON CONFLICT ")
                .push(target)
                .push(" DO NOTHING");
        }
        insert.push(" RETURNING id");

        let returned = insert
            .build_query_as::<InsertedId>()
            .fetch_all(&self.dest)
            .await?;

        if spec.on_conflict.is_some() {
            return Ok(Vec::new());
        }

        Ok(to_insert
            .iter()
            .map(|r| r.local_id)
            .zip(returned.into_iter().map(|row| row.id))
            .collect())
    }
}
