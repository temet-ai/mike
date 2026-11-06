use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct DriftSums {
    pub n_current: i64,
    pub current_sum: Option<String>,
    pub n_baseline: i64,
    pub baseline_sum: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewDriftRow<'a> {
    pub day: NaiveDate,
    pub stream: &'a str,
    pub embedder_key: &'a str,
    pub drift_version: &'a str,
    pub n_current: i32,
    pub n_baseline: i32,
    pub distance: Option<f64>,
    pub z: Option<f64>,
    pub prior_days: i32,
    pub prior_mean: Option<f64>,
    pub prior_sd: Option<f64>,
    pub status: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct DriftRow {
    pub id: i64,
    pub day: NaiveDate,
    pub stream: String,
    pub embedder_key: String,
    pub drift_version: String,
    pub n_current: i32,
    pub n_baseline: i32,
    pub distance: Option<f64>,
    pub z: Option<f64>,
    pub prior_days: i32,
    pub prior_mean: Option<f64>,
    pub prior_sd: Option<f64>,
    pub status: String,
    pub computed_at: DateTime<Utc>,
}

impl PersistHandle {
    pub async fn daily_drift_sums(
        &self,
        stream: &str,
        embedder_key: &str,
        day_start: DateTime<Utc>,
    ) -> Result<DriftSums, PersistError> {
        let kind = match stream {
            "prompt" => Some("turn_prompt"),
            "ending" => Some("turn_ending"),
            "correction" => None,
            _ => {
                return Err(PersistError::DataInvariant(format!(
                    "unknown drift stream {stream}"
                )));
            }
        };
        crate::metrics::time_db("daily_drift_sums", async move {
            Ok(match kind {
                Some(kind) => {
                    sqlx::query_file_as!(
                        DriftSums,
                        "src/store/persistence/semantic/drift_turn_sums.sql",
                        embedder_key,
                        day_start,
                        kind,
                    )
                    .fetch_one(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        DriftSums,
                        "src/store/persistence/semantic/drift_correction_sums.sql",
                        embedder_key,
                        day_start,
                    )
                    .fetch_one(&self.pool)
                    .await?
                }
            })
        })
        .await
    }

    pub async fn prior_drift_distances(
        &self,
        stream: &str,
        embedder_key: &str,
        drift_version: &str,
        day: NaiveDate,
        limit: i64,
    ) -> Result<Vec<f64>, PersistError> {
        crate::metrics::time_db("prior_drift_distances", async move {
            let rows = sqlx::query_scalar!(
                "SELECT distance FROM eval.embedding_drift
                  WHERE stream = $1 AND embedder_key = $2 AND drift_version = $3 AND day < $4
                    AND status = 'scored'
                  ORDER BY day DESC
                  LIMIT $5",
                stream,
                embedder_key,
                drift_version,
                day,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().flatten().collect())
        })
        .await
    }

    pub async fn insert_drift_row(&self, r: &NewDriftRow<'_>) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("insert_drift_row", async move {
            Ok(sqlx::query_scalar!(
                "INSERT INTO eval.embedding_drift
                    (day, stream, embedder_key, drift_version, n_current, n_baseline, distance, z,
                     prior_days, prior_mean, prior_sd, status)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT (day, stream, embedder_key, drift_version) DO NOTHING
                 RETURNING id",
                r.day,
                r.stream,
                r.embedder_key,
                r.drift_version,
                r.n_current,
                r.n_baseline,
                r.distance,
                r.z,
                r.prior_days,
                r.prior_mean,
                r.prior_sd,
                r.status,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_drift_rows(
        &self,
        stream: Option<&str>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<DriftRow>, PersistError> {
        crate::metrics::time_db("list_drift_rows", async move {
            let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
                "SELECT id, day, stream, embedder_key, drift_version, n_current, n_baseline,
                        distance, z, prior_days, prior_mean, prior_sd, status, computed_at
                   FROM eval.embedding_drift WHERE true",
            );
            if let Some(stream) = stream {
                query.push(" AND stream = ").push_bind(stream);
            }
            if let Some(from) = from {
                query.push(" AND day >= ").push_bind(from);
            }
            if let Some(to) = to {
                query.push(" AND day <= ").push_bind(to);
            }
            query.push(" ORDER BY day, stream");
            Ok(query
                .build_query_as::<DriftRow>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }
}
