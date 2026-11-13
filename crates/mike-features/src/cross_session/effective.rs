use serde::Serialize;

use super::snapshot::CrossSessionSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ThresholdSource {
    Static,
    Calibrated { row_id: i64, project: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ThresholdChoice {
    pub value: f64,
    pub source: ThresholdSource,
    pub shadow_value: Option<f64>,
}

pub fn effective_threshold(
    snapshot: &CrossSessionSnapshot,
    question: &str,
    project_id: Option<i64>,
    static_threshold: f64,
) -> ThresholdChoice {
    let find = |scope: &str, project: Option<i64>| {
        snapshot
            .thresholds
            .iter()
            .find(|t| t.question == question && t.scope == scope && t.project_id == project)
    };
    let chosen = project_id
        .and_then(|p| find("project", Some(p)))
        .or_else(|| find("global", None));
    let applies = snapshot.settings.threshold_mode == "apply";
    match chosen {
        Some(t) if applies => ThresholdChoice {
            value: f64::from(t.applied_threshold),
            source: ThresholdSource::Calibrated {
                row_id: t.id,
                project: t.scope == "project",
            },
            shadow_value: None,
        },
        Some(t) => ThresholdChoice {
            value: static_threshold,
            source: ThresholdSource::Static,
            shadow_value: Some(f64::from(t.applied_threshold)),
        },
        None => ThresholdChoice {
            value: static_threshold,
            source: ThresholdSource::Static,
            shadow_value: None,
        },
    }
}

pub fn effective_fire_threshold(
    snapshot: &CrossSessionSnapshot,
    steerer_id: i64,
    static_threshold: f32,
) -> f32 {
    if snapshot.settings.threshold_mode != "apply" {
        return static_threshold;
    }
    snapshot
        .fire_thresholds
        .iter()
        .find(|t| t.steerer_id == steerer_id)
        .map_or(static_threshold, |t| t.applied_threshold)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::cross_session::ThresholdRow;

    fn row(id: i64, scope: &str, project: Option<i64>, value: f32) -> ThresholdRow {
        ThresholdRow {
            id,
            steerer_id: 1,
            question: "relapse".to_owned(),
            scope: scope.to_owned(),
            project_id: project,
            computed_at: chrono::Utc::now(),
            rollup_id: 1,
            applied_threshold: value,
            decision: "applied".to_owned(),
            detail: serde_json::json!({}),
        }
    }

    #[test]
    fn effective_fire_threshold_uses_the_calibrated_value_only_in_apply_mode() {
        let mut s = CrossSessionSnapshot {
            fire_thresholds: vec![ThresholdRow {
                steerer_id: 6,
                question: "fire".to_owned(),
                ..row(1, "global", None, 0.88)
            }],
            ..CrossSessionSnapshot::default()
        };
        assert_eq!(effective_fire_threshold(&s, 6, 0.7), 0.7);
        s.settings.threshold_mode = "apply".to_owned();
        assert_eq!(effective_fire_threshold(&s, 6, 0.7), 0.88);
        assert_eq!(effective_fire_threshold(&s, 15, 0.7), 0.7);
    }

    #[test]
    fn effective_threshold_prefers_project_then_global_then_static() {
        let mut s = CrossSessionSnapshot::default();
        s.settings.threshold_mode = "apply".to_owned();
        s.thresholds = vec![row(1, "global", None, 0.8), row(2, "project", Some(7), 0.9)];
        let p = effective_threshold(&s, "relapse", Some(7), 0.75);
        assert!((p.value - 0.9).abs() < 1e-6);
        let g = effective_threshold(&s, "relapse", Some(8), 0.75);
        assert!((g.value - 0.8).abs() < 1e-6);
        let none = effective_threshold(&s, "unbacked_wait", Some(7), 0.75);
        assert_eq!(none.source, ThresholdSource::Static);
        s.settings.threshold_mode = "off".to_owned();
        let off = effective_threshold(&s, "relapse", Some(7), 0.75);
        assert_eq!(off.value, 0.75);
        assert!(off.shadow_value.is_some());
    }
}
