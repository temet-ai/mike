use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use serde::Serialize;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_metrics::{AlarmKind, Firing};
use crate::store::persistence::semantic::{EmbedderRow, NewDriftRow, parse_vector};

use super::SemanticError;
use super::config::SemanticConfig;
use super::drift::{
    DRIFT_VERSION, DayInputs, DriftPoint, PRIOR_DAYS, STREAMS, fires, min_current_for, score_day,
};

const CLEAR_AFTER_QUIET_DAYS: i32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DriftLine {
    pub day: NaiveDate,
    pub stream: &'static str,
    pub n_current: i64,
    pub n_baseline: i64,
    pub point: DriftPoint,
    pub fired: bool,
}

pub fn day_start(day: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&day.and_time(NaiveTime::MIN))
}

pub struct DriftRun<'a> {
    pub persist: &'a PersistHandle,
    pub config: &'a SemanticConfig,
    pub embedder: &'a EmbedderRow,
    pub write: bool,
}

impl DriftRun<'_> {
    async fn stream_day(
        &self,
        stream: &'static str,
        day: NaiveDate,
    ) -> Result<DriftLine, SemanticError> {
        let key = &self.embedder.embedder_key;
        let sums = self
            .persist
            .daily_drift_sums(stream, key, day_start(day))
            .await?;
        let prior = self
            .persist
            .prior_drift_distances(
                stream,
                key,
                DRIFT_VERSION,
                day,
                i64::try_from(PRIOR_DAYS).unwrap_or(28),
            )
            .await?;
        let current = sums.current_sum.as_deref().map(parse_vector);
        let baseline = sums.baseline_sum.as_deref().map(parse_vector);
        let age = (day_start(day) - self.embedder.first_seen_at).num_days();
        let point = score_day(&DayInputs {
            current_sum: current.as_deref(),
            baseline_sum: baseline.as_deref(),
            n_current: sums.n_current,
            min_current: min_current_for(stream, self.config.drift_min_current),
            prior: &prior,
            embedder_age_days: age,
        });
        if self.write {
            self.persist
                .insert_drift_row(&NewDriftRow {
                    day,
                    stream,
                    embedder_key: key,
                    drift_version: DRIFT_VERSION,
                    n_current: i32::try_from(sums.n_current).unwrap_or(i32::MAX),
                    n_baseline: i32::try_from(sums.n_baseline).unwrap_or(i32::MAX),
                    distance: point.distance,
                    z: point.z,
                    prior_days: i32::try_from(point.prior_days).unwrap_or(0),
                    prior_mean: point.prior_mean,
                    prior_sd: point.prior_sd,
                    status: point.status.code(),
                })
                .await?;
        }
        Ok(DriftLine {
            day,
            stream,
            n_current: sums.n_current,
            n_baseline: sums.n_baseline,
            fired: fires(&point, self.config.drift_z),
            point,
        })
    }

    pub async fn day(&self, day: NaiveDate) -> Result<Vec<DriftLine>, SemanticError> {
        let mut lines = Vec::new();
        for stream in STREAMS {
            lines.push(self.stream_day(stream, day).await?);
        }
        Ok(lines)
    }

    pub async fn raise_alarms(&self, lines: &[DriftLine]) -> Result<(), SemanticError> {
        let firings: Vec<Firing> = lines
            .iter()
            .filter(|l| l.fired)
            .map(|l| Firing {
                kind: AlarmKind::EmbeddingDrift,
                subject: format!("embedding:{}", l.stream),
                detail: serde_json::json!({
                    "day": l.day,
                    "embedder_key": self.embedder.embedder_key,
                    "distance": l.point.distance,
                    "z": l.point.z,
                    "n_current": l.n_current,
                    "n_baseline": l.n_baseline,
                }),
            })
            .collect();
        self.persist
            .apply_alarm_pass(
                &[AlarmKind::EmbeddingDrift],
                &firings,
                CLEAR_AFTER_QUIET_DAYS,
            )
            .await?;
        Ok(())
    }
}
