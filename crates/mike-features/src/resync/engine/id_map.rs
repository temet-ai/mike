use std::collections::HashMap;

use sqlx::{FromRow, Postgres, QueryBuilder};

use super::{ResyncEngine, ResyncError, TableSpec};

#[derive(FromRow)]
struct DeferredFkPair {
    id: i64,
    fk: Option<i64>,
}

impl ResyncEngine {
    pub async fn load_id_map(&self, table: &str) -> Result<HashMap<i64, i64>, ResyncError> {
        let rows = sqlx::query!(
            "SELECT local_id, remote_id FROM sync.id_map WHERE table_name = $1",
            table
        )
        .fetch_all(&self.source)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.local_id, row.remote_id))
            .collect())
    }

    pub async fn record_id_map(
        &self,
        table: &str,
        mappings: &[(i64, i64)],
    ) -> Result<(), ResyncError> {
        let locals: Vec<i64> = mappings.iter().map(|m| m.0).collect();
        let remotes: Vec<i64> = mappings.iter().map(|m| m.1).collect();
        sqlx::query!(
            "INSERT INTO sync.id_map (table_name, local_id, remote_id)
             SELECT $1, m.l, m.r FROM unnest($2::bigint[], $3::bigint[]) AS m(l, r)
             ON CONFLICT (table_name, local_id) DO NOTHING",
            table,
            &locals,
            &remotes,
        )
        .execute(&self.source)
        .await?;
        Ok(())
    }

    pub async fn fixup_deferred_fks(&self, spec: &TableSpec) -> Result<(), ResyncError> {
        let self_map = self.load_id_map(spec.name).await?;
        if self_map.is_empty() {
            return Ok(());
        }

        for fk in spec.deferred_fks() {
            let parent = fk.references.unwrap_or(spec.name);
            let parent_map = if parent == spec.name {
                self_map.clone()
            } else {
                self.load_id_map(parent).await?
            };

            let mut select = QueryBuilder::<Postgres>::new("SELECT id, ");
            select
                .push(fk.column)
                .push(" AS fk FROM ")
                .push(spec.name)
                .push(" WHERE ")
                .push(fk.column)
                .push(" IS NOT NULL");
            let pairs = select
                .build_query_as::<DeferredFkPair>()
                .fetch_all(&self.source)
                .await?;

            let mut remote_rows: Vec<i64> = Vec::new();
            let mut remote_targets: Vec<i64> = Vec::new();
            for pair in pairs {
                let Some(local_fk) = pair.fk else { continue };
                let (Some(remote_row), Some(remote_target)) =
                    (self_map.get(&pair.id), parent_map.get(&local_fk))
                else {
                    continue;
                };
                remote_rows.push(*remote_row);
                remote_targets.push(*remote_target);
            }
            if remote_rows.is_empty() {
                continue;
            }

            let mut update = QueryBuilder::<Postgres>::new("UPDATE ");
            update
                .push(spec.name)
                .push(" AS t SET ")
                .push(fk.column)
                .push(" = u.target FROM unnest(")
                .push_bind(&remote_rows)
                .push("::bigint[], ")
                .push_bind(&remote_targets)
                .push("::bigint[]) AS u(rowid, target) JOIN ")
                .push(parent)
                .push(" AS p ON p.id = u.target WHERE t.id = u.rowid");
            update.build().execute(&self.dest).await?;
        }
        Ok(())
    }
}
