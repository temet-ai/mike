use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct RollupRef {
    pub id: i64,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct CalibrationRateRow {
    pub family: String,
    pub metric: String,
    pub population: String,
    pub scope_kind: String,
    pub project_id: Option<i64>,
    pub axis: String,
    pub class: String,
    pub dimension: Option<String>,
    pub dimension_value: Option<String>,
    pub question: Option<String>,
    pub mode: Option<String>,
    pub k: i32,
    pub n: i32,
    pub value: Option<f64>,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub detail: serde_json::Value,
}

pub fn rate_key(
    name: &str,
) -> Result<(&'static str, &'static str, &'static str, &'static str), String> {
    match name {
        "corrected_share" => Ok(("base_rate", "replied_real_turns", "reaction", "corrected")),
        "frustration_close_share" => Ok((
            "base_rate",
            "real_session_ends",
            "session_end",
            "closed_in_frustration",
        )),
        "candidate_precision" => Ok((
            "steer_calibration",
            "steerer_evaluations",
            "steer_candidate",
            "applies",
        )),
        other => Err(format!("unknown calibration rate {other:?}")),
    }
}

impl PersistHandle {
    pub async fn rollup_as_of(
        &self,
        as_of: DateTime<Utc>,
    ) -> Result<Option<RollupRef>, PersistError> {
        crate::metrics::time_db("rollup_as_of", async move {
            Ok(sqlx::query_as!(
                RollupRef,
                r#"SELECT r.id, r.finished_at AS "finished_at!" FROM eval.rollups r
                    WHERE r.finished_at IS NOT NULL AND r.finished_at <= $1
                      AND EXISTS (SELECT 1 FROM eval.rates x
                                   WHERE x.rollup_id = r.id AND x.family = 'steer_calibration')
                    ORDER BY r.finished_at DESC, r.id DESC LIMIT 1"#,
                as_of,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fetch_calibration_rows(
        &self,
        rollup_id: i64,
        steerer_id: i64,
    ) -> Result<Vec<CalibrationRateRow>, PersistError> {
        crate::metrics::time_db("fetch_calibration_rows", async move {
            Ok(sqlx::query_as!(
                CalibrationRateRow,
                "SELECT family, metric, population, scope_kind, project_id, axis, class, dimension,
                        dimension_value, question, mode, k, n, value, ci_low, ci_high, detail
                   FROM eval.rates
                  WHERE rollup_id = $1
                    AND ((family = 'steer_calibration' AND axis = 'steer_candidate' AND steerer_id = $2)
                         OR (family = 'base_rate' AND metric = 'share'
                             AND (dimension IS NULL OR dimension IN ('ended_on', 'guard_fact'))
                             AND (dimension_value IS NULL
                                  OR dimension_value IN ('all', 'after_corrected_turn',
                                                         'unobserved_after_change'))))
                  ORDER BY family, scope_kind, project_id, question, dimension, dimension_value",
                rollup_id,
                steerer_id,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
