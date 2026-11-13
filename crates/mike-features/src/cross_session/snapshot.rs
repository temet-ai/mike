use crate::store::persistence::steerer_repo::SteererPersist as _;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::settings::cross_session::CrossSessionSettings;
use crate::store::persistence::cross_session::{CalibrationRateRow, RollupRef, ThresholdRow};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CrossSessionSnapshot {
    pub as_of: Option<DateTime<Utc>>,
    pub guard_steerer_id: Option<i64>,
    pub rollup: Option<RollupRef>,
    pub rows: Vec<CalibrationRateRow>,
    pub thresholds: Vec<ThresholdRow>,
    pub fire_thresholds: Vec<ThresholdRow>,
    pub settings: CrossSessionSettings,
}

impl CrossSessionSnapshot {
    pub fn describes(&self) -> serde_json::Value {
        serde_json::json!({
            "input_mode": self.settings.input_mode,
            "threshold_mode": self.settings.threshold_mode,
            "rollup_id": self.rollup.map(|r| r.id),
            "thresholds_in_force": self.thresholds.iter().map(|t| t.id).collect::<Vec<_>>(),
        })
    }

    pub async fn load_now(persist: &PersistHandle) -> Result<Self, PersistError> {
        let rows = persist.fetch_settings().await?;
        let settings = CrossSessionSettings::from_rows(
            rows.iter()
                .filter(|(k, _)| CrossSessionSettings::is_ours(k))
                .map(|(k, v)| (k.as_str(), v)),
        );
        Self::load_with(persist, Utc::now(), settings).await
    }

    pub async fn load_as_of(
        persist: &PersistHandle,
        as_of: DateTime<Utc>,
    ) -> Result<Self, PersistError> {
        let snapshot = persist.config_snapshot_at(as_of, Some("setting")).await?;
        let settings = CrossSessionSettings::from_rows(
            snapshot
                .entries
                .iter()
                .filter(|e| CrossSessionSettings::is_ours(&e.subject_key))
                .filter_map(|e| e.value.as_ref().map(|v| (e.subject_key.as_str(), v))),
        );
        Self::load_with(persist, as_of, settings).await
    }

    async fn load_with(
        persist: &PersistHandle,
        as_of: DateTime<Utc>,
        settings: CrossSessionSettings,
    ) -> Result<Self, PersistError> {
        let guard = persist.steerer().guard_steerer().await?.map(|g| g.id);
        let rollup = persist.steerer().rollup_as_of(as_of).await?;
        let (rows, thresholds) = match (rollup, guard) {
            (Some(r), Some(g)) => (
                persist.steerer().fetch_calibration_rows(r.id, g).await?,
                persist.steerer().thresholds_in_force(g, as_of).await?,
            ),
            (None, Some(g)) => (
                Vec::new(),
                persist.steerer().thresholds_in_force(g, as_of).await?,
            ),
            _ => (Vec::new(), Vec::new()),
        };
        let fire_thresholds = persist.steerer().fire_thresholds_in_force(as_of).await?;
        Ok(Self {
            as_of: Some(as_of),
            guard_steerer_id: guard,
            rollup,
            rows,
            thresholds,
            fire_thresholds,
            settings,
        })
    }

    pub fn candidate_row(
        &self,
        question: &str,
        project_id: Option<i64>,
    ) -> Option<&CalibrationRateRow> {
        self.rows.iter().find(|r| {
            r.family == "steer_calibration"
                && r.question.as_deref() == Some(question)
                && r.dimension.as_deref() == Some("all")
                && r.project_id == project_id
        })
    }

    pub fn base_rate(
        &self,
        axis: &str,
        dimension: Option<&str>,
        value: &str,
        project_id: Option<i64>,
    ) -> Option<&CalibrationRateRow> {
        self.rows.iter().find(|r| {
            r.family == "base_rate"
                && r.axis == axis
                && r.project_id == project_id
                && r.dimension.as_deref() == dimension
                && r.dimension_value.as_deref().is_none_or(|v| v == value)
        })
    }

    pub fn bins(&self, question: &str, project_id: Option<i64>) -> Vec<&CalibrationRateRow> {
        self.rows
            .iter()
            .filter(|r| {
                r.family == "steer_calibration"
                    && r.question.as_deref() == Some(question)
                    && r.dimension.as_deref() == Some("confidence_bin_05")
                    && r.project_id == project_id
            })
            .collect()
    }
}
