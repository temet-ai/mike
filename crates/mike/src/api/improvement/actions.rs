use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::AppState;
use crate::steerer::guard::Question;
use crate::steerer::guard::promotion::effect::{
    EffectEstimate, READ_POINT_HOLDBACK, estimate_effect,
};
use crate::store::persistence::PersistError;
use crate::store::persistence::improvement::actions::{GuardQuestionActionRow, LessonActionRow};
use crate::store::persistence::improvement::interval::{
    EffectReadiness, READ_POINT_N, effect_readiness,
};

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ActionsQuery {
    pub project: Option<String>,
    #[serde(default)]
    pub include_exposures: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PeriodStats {
    pub label: &'static str,
    pub k: i64,
    pub n: i64,
    pub share: f64,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
}

fn period(
    label: &'static str,
    k: i64,
    n: i64,
    window: (Option<DateTime<Utc>>, Option<DateTime<Utc>>),
) -> PeriodStats {
    PeriodStats {
        label,
        k,
        n,
        share: if n > 0 { k as f64 / n as f64 } else { 0.0 },
        window_start: window.0,
        window_end: window.1,
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum EffectView {
    NotComputed,
    NotEnoughData {
        n_before: i64,
        n_after: i64,
        read_point: i64,
    },
    Ready {
        diff: f64,
        ci_low: f64,
        ci_high: f64,
        read_point: i64,
        before: PeriodStats,
        after: PeriodStats,
    },
    Randomised {
        measure: &'static str,
        measure_status: &'static str,
        n_holdback: i64,
        n_treated: i64,
        read_point: i64,
        rd: Option<f64>,
        ci_low: Option<f64>,
        ci_high: Option<f64>,
        detail_endpoint: &'static str,
    },
}

struct PeriodInput {
    label: &'static str,
    k: i64,
    n: i64,
    window_start: Option<DateTime<Utc>>,
    window_end: Option<DateTime<Utc>>,
}

fn effect_view(before: PeriodInput, after: PeriodInput) -> EffectView {
    match effect_readiness(before.k, before.n, after.k, after.n) {
        EffectReadiness::NotEnoughData {
            n_before,
            n_after,
            read_point,
        } => EffectView::NotEnoughData {
            n_before,
            n_after,
            read_point,
        },
        EffectReadiness::Ready(interval) => EffectView::Ready {
            diff: interval.diff,
            ci_low: interval.low,
            ci_high: interval.high,
            read_point: READ_POINT_N,
            before: period(
                before.label,
                before.k,
                before.n,
                (before.window_start, before.window_end),
            ),
            after: period(
                after.label,
                after.k,
                after.n,
                (after.window_start, after.window_end),
            ),
        },
    }
}

pub(crate) fn lesson_effect(row: &LessonActionRow) -> EffectView {
    if row.n_before.is_none() || row.n_after.is_none() {
        return EffectView::NotComputed;
    }
    effect_view(
        PeriodInput {
            label: "before",
            k: row.k_before.map(i64::from).unwrap_or(0),
            n: row.n_before.map(i64::from).unwrap_or(0),
            window_start: row.window_start_before,
            window_end: row.window_end_before,
        },
        PeriodInput {
            label: "after",
            k: row.k_after.map(i64::from).unwrap_or(0),
            n: row.n_after.map(i64::from).unwrap_or(0),
            window_start: row.window_start_after,
            window_end: row.window_end_after,
        },
    )
}

fn guard_read_point() -> i64 {
    i64::try_from(READ_POINT_HOLDBACK).unwrap_or(i64::MAX)
}

fn guard_fallback(row: &GuardQuestionActionRow) -> EffectView {
    EffectView::Randomised {
        measure: "holdback_estimate",
        measure_status: "monitoring",
        n_holdback: row.n_holdback.map(i64::from).unwrap_or(0),
        n_treated: row.n_treated.map(i64::from).unwrap_or(0),
        read_point: guard_read_point(),
        rd: None,
        ci_low: None,
        ci_high: None,
        detail_endpoint: "/api/steering/guard/effect",
    }
}

async fn guard_estimate(
    persist: &crate::store::persistence::PersistHandle,
    steerer_id: i64,
    question: Question,
) -> Result<EffectEstimate, PersistError> {
    let now = Utc::now();
    let rows = persist
        .fetch_arm_rows(steerer_id, Some(question.code()), now)
        .await?;
    let turns: Vec<i64> = rows
        .iter()
        .filter(|r| r.scored_is_last)
        .filter_map(|r| r.scored_turn_id)
        .collect();
    let ends: HashMap<i64, String> = persist
        .session_end_labels_for_turns(&turns, now)
        .await?
        .into_iter()
        .map(|r| (r.end_turn_id, r.label))
        .collect();
    Ok(estimate_effect(
        &rows,
        &ends,
        Some(question.code()),
        now.timestamp(),
    ))
}

async fn guard_effect(
    guard_ctx: Option<(&crate::store::persistence::PersistHandle, i64)>,
    row: &GuardQuestionActionRow,
) -> EffectView {
    let Some((persist, steerer_id)) = guard_ctx else {
        return guard_fallback(row);
    };
    let Some(question) = Question::ALL.into_iter().find(|q| q.code() == row.question) else {
        return guard_fallback(row);
    };
    match guard_estimate(persist, steerer_id, question).await {
        Ok(estimate) => EffectView::Randomised {
            measure: "holdback_estimate",
            measure_status: estimate.status,
            n_holdback: i64::try_from(estimate.primary.holdback.n).unwrap_or(0),
            n_treated: i64::try_from(estimate.primary.treated.n).unwrap_or(0),
            read_point: guard_read_point(),
            rd: estimate.primary.rd,
            ci_low: estimate.primary.ci.map(|c| c.0),
            ci_high: estimate.primary.ci.map(|c| c.1),
            detail_endpoint: "/api/steering/guard/effect",
        },
        Err(_) => guard_fallback(row),
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ActionFeedItem {
    Lesson {
        id: i64,
        text: String,
        stage: String,
        source: String,
        created_at: DateTime<Utc>,
        project: Option<String>,
        effect: EffectView,
    },
    GuardQuestion {
        id: String,
        mode: String,
        effect: EffectView,
    },
}

fn lesson_items(rows: &[LessonActionRow]) -> Vec<ActionFeedItem> {
    rows.iter()
        .map(|l| ActionFeedItem::Lesson {
            id: l.lesson_id,
            text: l.text.clone(),
            stage: l.stage.clone(),
            source: l.source.clone(),
            created_at: l.created_at,
            project: l.project_path.clone(),
            effect: lesson_effect(l),
        })
        .collect()
}

async fn guard_items(
    persist: &crate::store::persistence::PersistHandle,
    steerer_id: Option<i64>,
    rows: &[GuardQuestionActionRow],
) -> Vec<ActionFeedItem> {
    let mut items = Vec::with_capacity(rows.len());
    for g in rows {
        let ctx = steerer_id.map(|id| (persist, id));
        let effect = guard_effect(ctx, g).await;
        items.push(ActionFeedItem::GuardQuestion {
            id: g.question.clone(),
            mode: g.mode.clone(),
            effect,
        });
    }
    items
}

pub(crate) async fn list_actions(
    State(state): State<AppState>,
    Query(q): Query<ActionsQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let lessons = match persist.list_lesson_actions(q.project.as_deref()).await {
        Ok(rows) => rows,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    let mut items = lesson_items(&lessons);
    if q.project.is_none() {
        let guard_questions = match persist.list_guard_question_actions().await {
            Ok(rows) => rows,
            Err(e) => {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("persistence error: {e}"),
                );
            }
        };
        let steerer_id = persist.guard_steerer().await.ok().flatten().map(|r| r.id);
        items.extend(guard_items(persist, steerer_id, &guard_questions).await);
    }
    let exposures = if q.include_exposures {
        let until = Utc::now();
        match persist
            .list_lesson_exposures(
                q.project.as_deref(),
                until - chrono::Duration::days(7),
                until,
            )
            .await
        {
            Ok(rows) => {
                json!({"status":"available", "measure":"recorded_exposure_association", "until":until, "days":7, "lessons":rows})
            }
            Err(error) => json!({"status":"unavailable", "error":error.to_string()}),
        }
    } else {
        json!({"status":"not_requested", "endpoint":"/api/improvement/exposures"})
    };
    Json(json!({
        "actions": items,
        "live_exposures": exposures,
        "lesson_measure": {
            "basis": "curated_dev_reaction_labels_project_before_after",
            "causal": false,
            "zero_after_means": "no eligible curated labels in the after window, not zero deliveries or live judge outcomes",
            "live_exposure_endpoint": "/api/improvement/exposures",
            "limitations": "Project before/after labels do not establish exposure to a lesson. The exposure endpoint separately reports recorded injections, live outcomes and usage; those associations are not causal estimates."
        }
    })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lesson_row(k_before: i32, n_before: i32, k_after: i32, n_after: i32) -> LessonActionRow {
        LessonActionRow {
            lesson_id: 1,
            text: "example".to_owned(),
            stage: "daily".to_owned(),
            source: "directive".to_owned(),
            created_at: Utc::now(),
            project_path: Some("proj".to_owned()),
            k_before: Some(k_before),
            n_before: Some(n_before),
            window_start_before: Some(Utc::now()),
            window_end_before: Some(Utc::now()),
            k_after: Some(k_after),
            n_after: Some(n_after),
            window_start_after: Some(Utc::now()),
            window_end_after: Some(Utc::now()),
        }
    }

    fn guard_row(
        k_holdback: i32,
        n_holdback: i32,
        k_treated: i32,
        n_treated: i32,
    ) -> GuardQuestionActionRow {
        GuardQuestionActionRow {
            question: "relapse".to_owned(),
            mode: "apply".to_owned(),
            k_holdback: Some(k_holdback),
            n_holdback: Some(n_holdback),
            window_start_holdback: Some(Utc::now()),
            window_end_holdback: Some(Utc::now()),
            k_treated: Some(k_treated),
            n_treated: Some(n_treated),
            window_start_treated: Some(Utc::now()),
            window_end_treated: Some(Utc::now()),
        }
    }

    #[test]
    fn lesson_effect_below_the_read_point_reports_not_enough_data() {
        let row = lesson_row(1, 5, 2, 10);
        match lesson_effect(&row) {
            EffectView::NotEnoughData {
                n_before, n_after, ..
            } => {
                assert_eq!((n_before, n_after), (5, 10));
            }
            other => panic!("expected NotEnoughData below the read point, got {other:?}"),
        }
    }

    #[test]
    fn lesson_effect_at_the_read_point_reports_a_newcombe_interval_with_its_read_point() {
        let row = lesson_row(8, 20, 2, 20);
        match lesson_effect(&row) {
            EffectView::Ready {
                diff,
                ci_low,
                ci_high,
                read_point,
                before,
                after,
            } => {
                assert!(diff < 0.0, "fewer corrections after: {diff}");
                assert!(ci_low <= diff && diff <= ci_high);
                assert_eq!(read_point, 20);
                assert_eq!(before.label, "before");
                assert_eq!((before.k, before.n), (8, 20));
                assert!((before.share - 0.4).abs() < 1e-9);
                assert!(before.window_start.is_some() && before.window_end.is_some());
                assert_eq!(after.label, "after");
                assert_eq!((after.k, after.n), (2, 20));
                assert!((after.share - 0.1).abs() < 1e-9);
                assert!(after.window_start.is_some() && after.window_end.is_some());
            }
            other => panic!("expected Ready at the read point, got {other:?}"),
        }
    }

    #[test]
    fn lesson_effect_distinguishes_missing_computation_from_empty_population() {
        let mut row = lesson_row(0, 0, 0, 0);
        assert!(matches!(
            lesson_effect(&row),
            EffectView::NotEnoughData { .. }
        ));
        row.k_before = None;
        row.n_before = None;
        assert!(matches!(lesson_effect(&row), EffectView::NotComputed));
    }

    #[test]
    fn guard_fallback_is_a_randomised_measure_never_a_before_after_share() {
        let row = guard_row(6, 20, 2, 20);
        match guard_fallback(&row) {
            EffectView::Randomised {
                measure,
                measure_status,
                n_holdback,
                n_treated,
                read_point,
                rd,
                ci_low,
                ci_high,
                detail_endpoint,
            } => {
                assert_eq!(measure, "holdback_estimate");
                assert_eq!(measure_status, "monitoring");
                assert_eq!((n_holdback, n_treated), (20, 20));
                assert_eq!(read_point, 34);
                assert!(rd.is_none() && ci_low.is_none() && ci_high.is_none());
                assert_eq!(detail_endpoint, "/api/steering/guard/effect");
            }
            other => panic!("expected Randomised for a guard question, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn guard_effect_falls_back_without_a_live_steerer() {
        let row = guard_row(6, 20, 2, 20);
        match guard_effect(None, &row).await {
            EffectView::Randomised {
                measure_status,
                rd,
                ci_low,
                ci_high,
                n_holdback,
                n_treated,
                ..
            } => {
                assert_eq!(measure_status, "monitoring");
                assert!(rd.is_none());
                assert!(ci_low.is_none() && ci_high.is_none());
                assert_eq!((n_holdback, n_treated), (20, 20));
            }
            other => panic!("expected Randomised, got {other:?}"),
        }
    }
}
