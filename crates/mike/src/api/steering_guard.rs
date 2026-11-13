use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::state::AppState;
use crate::steerer::guard::bars::promotion_bar;
use crate::steerer::guard::gates::GATE_VERSION;
use crate::steerer::guard::label::labeller_health;
use crate::steerer::guard::label_rules::GUARD_LABELLER_VERSION;
use crate::steerer::guard::metrics::{audit, question_metrics};
use crate::steerer::guard::policy::{
    GuardSettings, forget_policies, guard_config, question_modes, writable_mode,
};
use crate::steerer::guard::{GUARD_RUBRIC_VERSION, Question};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::guard::GuardMetricsWindow;

const AUDIT_WINDOW_DAYS: i64 = 14;

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; the turn guard is not available".to_owned(),
    )
}

fn persistence(e: &crate::store::persistence::PersistError) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}

async fn guard_id(persist: &PersistHandle) -> Result<(i64, bool), Response> {
    match persist.steerer().guard_steerer().await {
        Ok(Some(row)) => Ok((row.id, row.enabled)),
        Ok(None) => Err(error(
            StatusCode::NOT_FOUND,
            "the turn-guard steerer row is not seeded".to_owned(),
        )),
        Err(e) => Err(persistence(&e)),
    }
}

fn dependency_state(question: Question, turn_judge_live: bool) -> &'static str {
    match question {
        Question::Relapse if !turn_judge_live => "waiting on live reaction verdicts (JC12)",
        Question::Relapse => "live reaction verdicts available",
        Question::UnbackedWait => "wait backing from steering facts (JC15)",
        Question::AuthorisedHandback => "directives from the dossier (JC03)",
        Question::UnobservedDone => "observation after change from steering facts (JC15)",
    }
}

fn counts_24h(
    rows: &[crate::store::persistence::steering::guard::GuardMetricRow],
) -> BTreeMap<String, Value> {
    let mut out: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for r in rows.iter().filter(|r| !r.superseded) {
        let e = out
            .entry(r.question.clone().unwrap_or_default())
            .or_default();
        e.0 += 1;
        e.1 += usize::from(r.status == "resolved_correction");
    }
    out.into_iter()
        .map(|(q, (rows, fires))| (q, json!({"rows": rows, "would_fire": fires})))
        .collect()
}

pub(crate) async fn get_guard(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (steerer_id, steerer_enabled) = match guard_id(persist).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let modes = question_modes(persist, steerer_id).await;
    let config = guard_config(persist, steerer_id).await;
    let settings = GuardSettings::load(persist).await;
    let live = state.settings_overlay.load().turn_judge_live_bundle_id > 0;
    let now = Utc::now();
    let window = GuardMetricsWindow {
        steerer_id,
        since: now - Duration::hours(24),
        until: now,
        question: None,
    };
    let rows = match persist.steerer().guard_metric_rows(&window).await {
        Ok(rows) => rows,
        Err(e) => return persistence(&e),
    };
    let counts = counts_24h(&rows);
    let questions: Vec<Value> = Question::ALL
        .iter()
        .map(|q| {
            let p = config.questions.get(q);
            json!({
                "question": q.code(),
                "mode": modes.get(q).map_or("shadow", |m| m.code()),
                "confidence_threshold": p.map(|p| p.confidence_threshold),
                "floor": p.map(|p| p.floor),
                "bar": promotion_bar(*q),
                "dependency_state": dependency_state(*q, live),
                "counts_24h": counts.get(q.code()).cloned().unwrap_or_else(|| json!({"rows": 0, "would_fire": 0})),
            })
        })
        .collect();
    Json(json!({
        "enabled": state.feature_toggle_overlay.load().turn_guard,
        "steerer_id": steerer_id,
        "steerer_enabled": steerer_enabled,
        "rubric_version": GUARD_RUBRIC_VERSION,
        "gate_version": GATE_VERSION,
        "steering_facts_version": crate::dossier::steering_facts::STEERING_FACTS_VERSION,
        "labeller_version": GUARD_LABELLER_VERSION,
        "labeller": labeller_health().await,
        "settings": settings,
        "questions": questions,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModeBody {
    pub mode: String,
}

pub(crate) async fn patch_guard_question(
    State(state): State<AppState>,
    Path(question): Path<String>,
    Json(body): Json<ModeBody>,
) -> Response {
    let Some(q) = Question::parse(&question) else {
        return error(
            StatusCode::BAD_REQUEST,
            format!(
                "question {question:?} is not one of relapse|unbacked_wait|authorised_handback|unobserved_done"
            ),
        );
    };
    let mode = match writable_mode(&body.mode) {
        Ok(mode) => mode,
        Err(message) if matches!(body.mode.as_str(), "append" | "wake") => {
            return error(StatusCode::CONFLICT, message);
        }
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (steerer_id, _) = match guard_id(persist).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if let Ok(Some(current)) = persist.steerer().policy_row(steerer_id, q.code()).await
        && matches!(current.mode.as_str(), "append" | "wake")
    {
        return error(
            StatusCode::CONFLICT,
            format!(
                "{} is {}; demote through POST /api/steering/questions/{steerer_id}/{}/demote",
                q.code(),
                current.mode,
                q.code()
            ),
        );
    }
    match persist
        .steerer()
        .set_question_mode(steerer_id, q.code(), mode.code(), "spa")
        .await
    {
        Ok(row) => {
            forget_policies(steerer_id).await;
            Json(row).into_response()
        }
        Err(e) => persistence(&e),
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct MetricsQuery {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub question: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MetricsView {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub metrics: Vec<crate::steerer::guard::metrics::QuestionMetrics>,
    pub audit: crate::steerer::guard::metrics::AuditReport,
}

pub(crate) async fn guard_metrics_view(
    persist: &PersistHandle,
    steerer_id: i64,
    q: &MetricsQuery,
) -> Result<MetricsView, crate::store::persistence::PersistError> {
    let until = q.until.unwrap_or_else(Utc::now);
    let since = q.since.unwrap_or(until - Duration::days(AUDIT_WINDOW_DAYS));
    let window = GuardMetricsWindow {
        steerer_id,
        since,
        until,
        question: q.question.clone(),
    };
    let rows = persist.steerer().guard_metric_rows(&window).await?;
    let audit_sessions = persist.steerer().guard_audit_sessions(since).await?;
    let settings = GuardSettings::load(persist).await;
    Ok(MetricsView {
        since,
        until,
        metrics: question_metrics(&rows, settings.recall_sample_pct),
        audit: audit(&rows, &audit_sessions),
    })
}

pub(crate) async fn get_guard_metrics(
    State(state): State<AppState>,
    Query(q): Query<MetricsQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (steerer_id, _) = match guard_id(persist).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match guard_metrics_view(persist, steerer_id, &q).await {
        Ok(view) => Json(view).into_response(),
        Err(e) => persistence(&e),
    }
}
