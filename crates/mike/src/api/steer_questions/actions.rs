use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::{error, parse_question, unavailable};
use crate::change_log::{ChangeVia, current_build};
use crate::config::AppState;
use crate::steerer::guard::policy::forget_policies;
use crate::steerer::guard::promotion::service::{
    DemoteRequest, PromoteRequest, PromotionError, demote, promote,
};
use crate::web_base::change_source::change_source_from_headers;

fn via(headers: &HeaderMap) -> (&'static str, String) {
    let source = change_source_from_headers(headers);
    let via = if source.via == ChangeVia::Spa {
        "spa"
    } else {
        "api"
    };
    (via, source.to_setting_json(&current_build()))
}

fn promotion_error(e: PromotionError) -> Response {
    match e {
        PromotionError::GateFailed(gate) => (StatusCode::CONFLICT, Json(json!({"error": "the gate fails; give override_reason to promote anyway", "gate": gate}))).into_response(),
        PromotionError::TargetForbidden(m) => error(StatusCode::UNPROCESSABLE_ENTITY, m),
        PromotionError::Refused(m) => error(StatusCode::CONFLICT, m),
        PromotionError::UnknownPolicy(m) => error(StatusCode::NOT_FOUND, format!("no policy row for {m}")),
        PromotionError::Persist(p) => error(StatusCode::INTERNAL_SERVER_ERROR, format!("persistence error: {p}")),
    }
}

async fn refresh_steerer(state: &AppState, headers: &HeaderMap, id: Option<i64>, enabled: bool) {
    let (Some(id), Some(service)) = (id, state.steerer_service()) else {
        return;
    };
    let source = change_source_from_headers(headers);
    if let Err(e) = service
        .set_enabled_preserving_config(id, enabled, &source)
        .await
    {
        tracing::warn!(steerer_id = id, error = %e, "steer questions: steerer reload failed");
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PromoteBody {
    pub to_mode: String,
    pub override_reason: Option<String>,
    pub replaces_steerer_id: Option<i64>,
}

pub(crate) async fn promote_question(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((steerer_id, question)): Path<(i64, String)>,
    Json(body): Json<PromoteBody>,
) -> Response {
    let question = match parse_question(&question) {
        Ok(q) => q,
        Err(resp) => return *resp,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (decided_via, source_json) = via(&headers);
    let req = PromoteRequest {
        steerer_id,
        question,
        to_mode: &body.to_mode,
        override_reason: body
            .override_reason
            .as_deref()
            .filter(|r| !r.trim().is_empty()),
        replaces_steerer_id: body.replaces_steerer_id,
        decided_via,
        bundle_id: state.settings_overlay.load().bundles.steerer(steerer_id),
        source_json: &source_json,
    };
    match promote(persist, &req).await {
        Ok((id, gate)) => {
            refresh_steerer(&state, &headers, body.replaces_steerer_id, false).await;
            Json(json!({"promotion_id": id, "gate": gate})).into_response()
        }
        Err(e) => promotion_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct DemoteBody {
    pub to_mode: String,
    pub reason: String,
    pub restore_steerer_id: Option<i64>,
}

pub(crate) async fn demote_question(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((steerer_id, question)): Path<(i64, String)>,
    Json(body): Json<DemoteBody>,
) -> Response {
    let question = match parse_question(&question) {
        Ok(q) => q,
        Err(resp) => return *resp,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (decided_via, source_json) = via(&headers);
    let req = DemoteRequest {
        steerer_id,
        question,
        to_mode: &body.to_mode,
        reason: &body.reason,
        restore_steerer_id: body.restore_steerer_id,
        decided_via,
        evidence: json!({"operator_reason": body.reason}),
        source_json: &source_json,
    };
    match demote(persist, &req).await {
        Ok(id) => {
            refresh_steerer(&state, &headers, body.restore_steerer_id, true).await;
            Json(json!({"promotion_id": id})).into_response()
        }
        Err(e) => promotion_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct HoldbackBody {
    pub holdback_pct: Option<i16>,
}

pub(crate) async fn patch_question(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((steerer_id, question)): Path<(i64, String)>,
    Json(body): Json<HoldbackBody>,
) -> Response {
    let question = match parse_question(&question) {
        Ok(q) => q,
        Err(resp) => return *resp,
    };
    if body.holdback_pct.is_some_and(|p| !(0..=50).contains(&p)) {
        return error(
            StatusCode::BAD_REQUEST,
            format!("holdback_pct {:?} is outside 0-50", body.holdback_pct),
        );
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (decided_via, _) = via(&headers);
    match persist
        .set_holdback_pct(steerer_id, question.code(), body.holdback_pct, decided_via)
        .await
    {
        Ok(true) => {
            forget_policies(steerer_id).await;
            Json(json!({"question": question.code(), "holdback_pct": body.holdback_pct}))
                .into_response()
        }
        Ok(false) => error(
            StatusCode::NOT_FOUND,
            format!("no policy for {}", question.code()),
        ),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct LabelBody {
    pub label: String,
    pub note: Option<String>,
}

pub(crate) async fn label_evaluation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(uuid): Path<uuid::Uuid>,
    Json(body): Json<LabelBody>,
) -> Response {
    if !matches!(body.label.as_str(), "right" | "wrong") {
        return error(
            StatusCode::BAD_REQUEST,
            format!("label {:?} is not right or wrong", body.label),
        );
    }
    if crate::api::eval::labels::spa_origin(&headers, &state).is_none() {
        return error(
            StatusCode::FORBIDDEN,
            "the operator's verdict on a fire is written from the SPA only (D2)".to_owned(),
        );
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let holdout = match crate::eval::labels::holdout_percent(persist).await {
        Ok(h) => h,
        Err(e) => return persistence_error(&e),
    };
    let label_id = match persist
        .label_steer_evaluation(uuid, &body.label, body.note.as_deref(), holdout)
        .await
    {
        Ok(Some(id)) => id,
        Ok(None) => return error(StatusCode::NOT_FOUND, format!("no evaluation {uuid}")),
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    let cancelled = if body.label == "wrong" {
        persist.reject_pending_wake(uuid).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    Json(json!({"label_id": label_id, "cancelled_wakes": cancelled})).into_response()
}

fn persistence_error(e: &crate::store::persistence::PersistError) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}
