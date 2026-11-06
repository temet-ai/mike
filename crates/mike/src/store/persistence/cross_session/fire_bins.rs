use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};
use super::ThresholdRow;

pub const FIRE_STEERERS: [&str; 2] = ["verifier", "monitor-over-sleep"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FireBinRow {
    pub steerer_id: i64,
    pub steerer_name: String,
    pub lower_edge: String,
    pub label_source: String,
    pub k: i32,
    pub n: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FireSteerer {
    pub id: i64,
    pub name: String,
    pub static_threshold: f32,
}

fn fire_steerer_names() -> Vec<String> {
    FIRE_STEERERS.iter().map(|n| (*n).to_owned()).collect()
}

impl PersistHandle {
    pub async fn fire_steerers(&self) -> Result<Vec<FireSteerer>, PersistError> {
        crate::metrics::time_db("fire_steerers", async move {
            let names = fire_steerer_names();
            Ok(sqlx::query_as!(
                FireSteerer,
                r#"SELECT id, name,
                          coalesce((config ->> 'confidence_threshold')::float8, 0.7)::float4 AS "static_threshold!"
                     FROM steering.steerers
                    WHERE name = ANY($1) AND deleted_at IS NULL
                    ORDER BY id"#,
                &names
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fetch_fire_bins(
        &self,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<FireBinRow>, PersistError> {
        crate::metrics::time_db("fetch_fire_bins", async move {
            let names = fire_steerer_names();
            Ok(sqlx::query_as!(
                FireBinRow,
                r#"SELECT e.steerer_id, st.name AS "steerer_name!",
                          to_char(least(floor(e.confidence * 20), 19) * 0.05, 'FM0.00') AS "lower_edge!",
                          e.label_source AS "label_source!",
                          (count(*) FILTER (WHERE e.outcome_label = 'right'))::int AS "k!",
                          count(*)::int AS "n!"
                     FROM steering.evaluations e
                     JOIN steering.steerers st ON st.id = e.steerer_id
                    WHERE st.name = ANY($2)
                      AND e.dispatched_at >= $1::timestamptz - interval '30 days' AND e.dispatched_at < $1
                      AND e.outcome_label IN ('right', 'wrong')
                      AND e.confidence IS NOT NULL
                      AND e.label_source IN ('operator', 'session_reply', 'session_end', 'next_message')
                    GROUP BY e.steerer_id, st.name, 3, e.label_source"#,
                window_end,
                &names
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn fire_thresholds_in_force(
        &self,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<ThresholdRow>, PersistError> {
        crate::metrics::time_db("fire_thresholds_in_force", async move {
            Ok(sqlx::query_as!(
                ThresholdRow,
                r#"SELECT DISTINCT ON (steerer_id)
                          id, steerer_id, question, scope, project_id, computed_at, rollup_id,
                          applied_threshold, decision, detail
                     FROM steering.guard_thresholds
                    WHERE question = 'fire' AND decision = 'applied' AND computed_at <= $1
                    ORDER BY steerer_id, computed_at DESC, id DESC"#,
                as_of
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
