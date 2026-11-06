use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FloorRow {
    pub id: i64,
    pub embedder_key: String,
    pub pair: String,
    pub quantile: f64,
    pub floor: f64,
    pub n_pairs: i64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p999: f64,
    pub sample: serde_json::Value,
    pub builder_version: String,
    pub status: String,
    pub promoted_from_id: Option<i64>,
    pub computed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FloorStats {
    pub n_pairs: i64,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub p999: Option<f64>,
    pub floor: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewFloor<'a> {
    pub embedder_key: &'a str,
    pub pair: &'a str,
    pub quantile: f64,
    pub stats: &'a FloorStats,
    pub sample: serde_json::Value,
    pub builder_version: &'a str,
    pub status: &'a str,
    pub promoted_from_id: Option<i64>,
}

const FACETS: [&str; 5] = ["prompt", "ending", "corr", "directive", "lesson"];

pub struct FloorSample<'a> {
    pub embedder_key: &'a str,
    pub pair: &'a str,
    pub seed: &'a str,
    pub sample_turns: i64,
    pub max_pairs: i64,
    pub quantile: f64,
    pub model: &'a str,
}

impl PersistHandle {
    pub async fn semantic_floors_in_force(
        &self,
        embedder_key: &str,
        quantile: f64,
    ) -> Result<Vec<FloorRow>, PersistError> {
        crate::metrics::time_db("semantic_floors_in_force", async move {
            Ok(sqlx::query_as!(
                FloorRow,
                "SELECT DISTINCT ON (pair) id, embedder_key, pair, quantile, floor, n_pairs,
                        p50, p95, p99, p999, sample, builder_version, status, promoted_from_id,
                        computed_at
                   FROM memory.semantic_floors
                  WHERE embedder_key = $1 AND status = 'in_force' AND quantile = $2
                  ORDER BY pair, id DESC",
                embedder_key,
                quantile,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn semantic_floors_latest_monitoring(
        &self,
        embedder_key: &str,
    ) -> Result<Vec<FloorRow>, PersistError> {
        crate::metrics::time_db("semantic_floors_latest_monitoring", async move {
            Ok(sqlx::query_as!(
                FloorRow,
                "SELECT DISTINCT ON (pair, quantile) id, embedder_key, pair, quantile, floor,
                        n_pairs, p50, p95, p99, p999, sample, builder_version, status,
                        promoted_from_id, computed_at
                   FROM memory.semantic_floors
                  WHERE embedder_key = $1 AND status = 'monitoring'
                  ORDER BY pair, quantile, id DESC",
                embedder_key,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn get_semantic_floor(&self, id: i64) -> Result<Option<FloorRow>, PersistError> {
        crate::metrics::time_db("get_semantic_floor", async move {
            Ok(sqlx::query_as!(
                FloorRow,
                "SELECT id, embedder_key, pair, quantile, floor, n_pairs, p50, p95, p99, p999,
                        sample, builder_version, status, promoted_from_id, computed_at
                   FROM memory.semantic_floors WHERE id = $1",
                id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn compute_semantic_floor(
        &self,
        s: &FloorSample<'_>,
    ) -> Result<FloorStats, PersistError> {
        let (a, b) = s
            .pair
            .split_once(':')
            .ok_or_else(|| PersistError::DataInvariant(format!("bad pair {}", s.pair)))?;
        if !FACETS.contains(&a) || !FACETS.contains(&b) {
            return Err(PersistError::DataInvariant(format!(
                "unknown facet in pair {}",
                s.pair
            )));
        }
        let seed_b = format!("{}-b", s.seed);
        crate::metrics::time_db("compute_floor", async move {
            Ok(sqlx::query_file_as!(
                FloorStats,
                "src/store/persistence/semantic/floor_pair.sql",
                s.embedder_key,
                s.seed,
                s.sample_turns,
                &seed_b,
                s.model,
                s.max_pairs,
                s.quantile,
                a,
                b,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn insert_semantic_floor(&self, f: &NewFloor<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_semantic_floor", async move {
            let st = f.stats;
            Ok(sqlx::query_scalar!(
                "INSERT INTO memory.semantic_floors
                    (embedder_key, pair, quantile, floor, n_pairs, p50, p95, p99, p999, sample,
                     builder_version, status, promoted_from_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 RETURNING id",
                f.embedder_key,
                f.pair,
                f.quantile,
                st.floor.unwrap_or(1.0),
                st.n_pairs.max(1),
                st.p50.unwrap_or(1.0),
                st.p95.unwrap_or(1.0),
                st.p99.unwrap_or(1.0),
                st.p999.unwrap_or(1.0),
                &f.sample,
                f.builder_version,
                f.status,
                f.promoted_from_id,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn semantic_floors_last_computed_at(
        &self,
        embedder_key: &str,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("semantic_floors_last_computed_at", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT max(computed_at) AS "last?" FROM memory.semantic_floors WHERE embedder_key = $1"#,
                embedder_key,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn semantic_key_has_floors(&self, embedder_key: &str) -> Result<bool, PersistError> {
        crate::metrics::time_db("semantic_key_has_floors", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT EXISTS (
                       SELECT 1 FROM memory.semantic_floors
                        WHERE embedder_key = $1 AND status = 'in_force') AS "exists!""#,
                embedder_key,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }
}
