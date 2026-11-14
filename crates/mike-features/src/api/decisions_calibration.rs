use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::llm::decisions::loop_store::PointCounts;
use crate::settings::SettingsOverlay;
use crate::settings::meta_decisions::{FLEET_POINTS, POINTS, area};
use crate::state::AppState;
use crate::steerer::decisions_loop::{self, PointScore};

#[derive(Debug, Serialize, PartialEq)]
pub struct LayerState {
    pub on: bool,
    pub reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct PointRow {
    pub point: String,
    pub label: Option<&'static str>,
    pub area: &'static str,
    pub mode: String,
    pub mode_source: &'static str,
    pub score: Option<PointScore>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SpendState {
    pub today_usd: f64,
    pub ceiling_usd: f64,
    pub reached: bool,
}

pub fn spend_state(today_usd: f64, ceiling_usd: f64) -> SpendState {
    SpendState {
        today_usd,
        ceiling_usd,
        reached: ceiling_usd > 0.0 && today_usd >= ceiling_usd,
    }
}

async fn decision_spend_today(
    persist: &crate::store::persistence::PersistHandle,
) -> Result<f64, crate::store::persistence::PersistError> {
    let day_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|naive| naive.and_utc());
    let rows = persist.llm_spend_by_role(day_start, None).await?;
    let decision = crate::llm::LlmRole::Decision.as_db_value();
    Ok(rows
        .iter()
        .filter(|r| Some(r.key.as_str()) == decision)
        .map(|r| r.cost_usd)
        .sum())
}

#[derive(Debug, Serialize)]
pub struct Calibration {
    pub layer: LayerState,
    pub spend: SpendState,
    pub window_days: i64,
    pub last_loop_at: Option<DateTime<Utc>>,
    pub points: Vec<PointRow>,
}

pub fn layer_state(toggle_on: bool, has_key: bool) -> LayerState {
    match (toggle_on, has_key) {
        (false, _) => LayerState {
            on: false,
            reason: Some("off: the decisions toggle is off"),
        },
        (true, false) => LayerState {
            on: false,
            reason: Some("off: no OpenRouter key"),
        },
        (true, true) => LayerState {
            on: true,
            reason: None,
        },
    }
}

fn mode_of(overlay: &SettingsOverlay, point: &str) -> (String, &'static str) {
    if point == crate::steerer::precheck::POINT {
        return (
            overlay.decisions_turn_guard_mode.clone(),
            "decisions.turn_guard.mode",
        );
    }
    if point.starts_with("fleet_") {
        return (
            "on".to_owned(),
            "fleet definition (decisions key, default on)",
        );
    }
    let mode = overlay
        .decisions_point_modes
        .get(point)
        .cloned()
        .unwrap_or_else(|| "on".to_owned());
    (mode, "setting")
}

fn row(overlay: &SettingsOverlay, point: &str, label: Option<&'static str>) -> PointRow {
    let (mode, mode_source) = mode_of(overlay, point);
    PointRow {
        point: point.to_owned(),
        label,
        area: area(point),
        mode,
        mode_source,
        score: None,
    }
}

pub fn rows(overlay: &SettingsOverlay, scores: Vec<PointScore>) -> Vec<PointRow> {
    let pre = std::iter::once((crate::steerer::precheck::POINT, "Turn-guard pre-check"));
    let mut out: Vec<PointRow> = pre
        .chain(POINTS.iter().map(|p| (p.point, p.label)))
        .chain(FLEET_POINTS.iter().map(|p| (p.point, p.label)))
        .map(|(point, label)| row(overlay, point, Some(label)))
        .collect();
    for score in scores {
        match out.iter_mut().find(|r| r.point == score.counts.point) {
            Some(r) => r.score = Some(score),
            None => {
                let mut r = row(overlay, &score.counts.point, None);
                r.score = Some(score);
                out.push(r);
            }
        }
    }
    out
}

async fn scores(
    persist: &crate::store::persistence::PersistHandle,
    since: DateTime<Utc>,
) -> Result<Vec<PointScore>, crate::store::persistence::PersistError> {
    let store = crate::llm::decisions::store(persist);
    let counts: Vec<PointCounts> = store.decision_point_counts(since).await?;
    let labelled = store.labelled_decisions(since).await?;
    Ok(counts
        .into_iter()
        .map(|c| decisions_loop::score_point(c, &labelled))
        .collect())
}

pub async fn get_calibration(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "persistence is disabled; decisions are not recorded" })),
        )
            .into_response();
    };
    let toggle_on = state.feature_toggle_overlay.load().decisions;
    let has_key = state.decision_env().is_some();
    let overlay = state.settings_overlay.load();
    let since = Utc::now() - decisions_loop::WINDOW;
    let result = async {
        let scored = scores(persist, since).await?;
        let last = crate::llm::decisions::store(persist)
            .last_decision_loop_at()
            .await?;
        let spent = decision_spend_today(persist).await?;
        Ok::<_, crate::store::persistence::PersistError>((scored, last, spent))
    }
    .await;
    match result {
        Ok((scored, last_loop_at, spent)) => Json(Calibration {
            layer: layer_state(toggle_on, has_key),
            spend: spend_state(spent, overlay.decisions_daily_spend_ceiling_usd),
            window_days: decisions_loop::WINDOW.num_days(),
            last_loop_at,
            points: rows(&overlay, scored),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_state_names_why_it_is_off() {
        assert_eq!(
            layer_state(false, true).reason,
            Some("off: the decisions toggle is off")
        );
        assert_eq!(
            layer_state(true, false).reason,
            Some("off: no OpenRouter key")
        );
        assert!(layer_state(true, true).on);
    }

    #[test]
    fn spend_state_is_reached_at_the_ceiling_and_never_when_the_ceiling_is_zero() {
        assert!(!spend_state(4.99, 5.0).reached);
        assert!(spend_state(5.0, 5.0).reached);
        assert!(!spend_state(500.0, 0.0).reached);
    }

    #[test]
    fn rows_list_every_registered_point_and_mark_unrun_ones_without_a_score() {
        let mut overlay = SettingsOverlay::default();
        overlay
            .decisions_point_modes
            .insert("turn_judge".to_owned(), "shadow".to_owned());
        let score = decisions_loop::score_point(
            PointCounts {
                point: "turn_guard_precheck".to_owned(),
                calls: 3,
                ..PointCounts::default()
            },
            &[],
        );
        let out = rows(&overlay, vec![score]);
        assert_eq!(out.len(), POINTS.len() + FLEET_POINTS.len() + 1);
        let judge = out.iter().find(|r| r.point == "turn_judge");
        assert_eq!(judge.map(|r| r.mode.as_str()), Some("shadow"));
        assert!(judge.is_some_and(|r| r.score.is_none() && r.area == "judges"));
        let pre = out.iter().find(|r| r.point == "turn_guard_precheck");
        assert!(pre.is_some_and(|r| r.score.is_some() && r.mode == "on"));
        let stop = out.iter().find(|r| r.point == "fleet_stop_check");
        assert!(stop.is_some_and(|r| r.area == "fleet" && r.mode_source.starts_with("fleet")));
    }
}
