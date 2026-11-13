use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::store::persistence::eval_metrics::{
    AckOutcome, AlarmState, AlarmsFilter, EvalAlarmRow, EvalRateRow, EvalRollupRow, RatesFilter,
};

const RATES_DEFAULT_LIMIT: i64 = 200;
const RATES_MAX_LIMIT: i64 = 1_000;
const ALARMS_DEFAULT_LIMIT: i64 = 100;
const ALARMS_MAX_LIMIT: i64 = 1_000;
const NOTE_MAX_CHARS: usize = 2_000;

#[derive(Debug, Deserialize, Default)]
pub struct RatesQuery {
    pub family: Option<String>,
    pub metric: Option<String>,
    pub population: Option<String>,
    pub role: Option<String>,
    pub bundle_id: Option<i64>,
    pub bundle_label: Option<String>,
    pub axis: Option<String>,
    pub class: Option<String>,
    pub label_tier: Option<String>,
    pub label_split: Option<String>,
    pub label_source: Option<String>,
    pub scope: Option<String>,
    pub project: Option<String>,
    pub window_days: Option<i32>,
    pub steerer_id: Option<i64>,
    pub question: Option<String>,
    pub dimension: Option<String>,
    pub rollup_id: Option<i64>,
    pub from_day: Option<NaiveDate>,
    pub to_day: Option<NaiveDate>,
    pub latest: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RatesPage {
    pub rollup: Option<EvalRollupRow>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub rows: Vec<EvalRateRow>,
}

#[derive(Debug, Deserialize, Default)]
pub struct AlarmsQuery {
    pub state: Option<String>,
    pub kind: Option<String>,
    pub acknowledged: Option<bool>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AlarmsPage {
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub alarms: Vec<EvalAlarmRow>,
}

#[derive(Debug, Deserialize)]
pub struct AcknowledgeBody {
    pub note: String,
    #[serde(default)]
    pub replace_note: bool,
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; evaluation metrics are not available".to_owned(),
    )
}

fn page(
    limit: Option<i64>,
    offset: Option<i64>,
    default: i64,
    max: i64,
) -> Result<(i64, i64), Box<Response>> {
    let limit = limit.unwrap_or(default);
    if !(1..=max).contains(&limit) {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {max}, got {limit}"),
        )));
    }
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("offset must be zero or more, got {offset}"),
        )));
    }
    Ok((limit, offset))
}

fn rates_filter(q: RatesQuery, limit: i64, offset: i64) -> RatesFilter {
    let latest = q
        .latest
        .unwrap_or(q.from_day.is_none() && q.to_day.is_none() && q.rollup_id.is_none());
    RatesFilter {
        family: q.family,
        metric: q.metric,
        population: q.population,
        role: q.role,
        bundle_id: q.bundle_id,
        bundle_label: q.bundle_label,
        axis: q.axis,
        class: q.class,
        label_tier: q.label_tier,
        label_split: q.label_split,
        label_source: q.label_source,
        scope: q.scope,
        project: q.project,
        window_days: q.window_days,
        steerer_id: q.steerer_id,
        question: q.question,
        dimension: q.dimension,
        rollup_id: q.rollup_id,
        from_day: q.from_day,
        to_day: q.to_day,
        latest,
        limit,
        offset,
    }
}

pub async fn get_eval_rates(
    State(state): State<AppState>,
    Query(query): Query<RatesQuery>,
) -> Response {
    let (limit, offset) = match page(
        query.limit,
        query.offset,
        RATES_DEFAULT_LIMIT,
        RATES_MAX_LIMIT,
    ) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = rates_filter(query, limit, offset);
    let rollup = match persist.eval().newest_eval_rollup().await {
        Ok(rollup) => rollup,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    match persist.eval().list_eval_rates(&filter).await {
        Ok((rows, total)) => Json(RatesPage {
            rollup,
            total,
            limit,
            offset,
            rows,
        })
        .into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

pub async fn list_eval_alarms(
    State(state): State<AppState>,
    Query(query): Query<AlarmsQuery>,
) -> Response {
    let (limit, offset) = match page(
        query.limit,
        query.offset,
        ALARMS_DEFAULT_LIMIT,
        ALARMS_MAX_LIMIT,
    ) {
        Ok(p) => p,
        Err(resp) => return *resp,
    };
    let alarm_state = match query.state.as_deref().map(AlarmState::parse) {
        None => AlarmState::Open,
        Some(Some(s)) => s,
        Some(None) => {
            return error(
                StatusCode::BAD_REQUEST,
                format!(
                    "state must be open, cleared or all; got {:?}",
                    query.state.unwrap_or_default()
                ),
            );
        }
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = AlarmsFilter {
        state: alarm_state,
        kind: query.kind,
        acknowledged: query.acknowledged,
        since: query.since,
        limit,
        offset,
    };
    match persist.eval().list_eval_alarms(&filter).await {
        Ok((alarms, total)) => Json(AlarmsPage {
            total,
            limit,
            offset,
            alarms,
        })
        .into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

pub async fn acknowledge_eval_alarm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AcknowledgeBody>,
) -> Response {
    let chars = body.note.chars().count();
    if chars == 0 || chars > NOTE_MAX_CHARS {
        return error(
            StatusCode::BAD_REQUEST,
            format!("note must be 1 to {NOTE_MAX_CHARS} characters, got {chars}"),
        );
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist
        .eval()
        .acknowledge_eval_alarm(id, &body.note, body.replace_note)
        .await
    {
        Ok((AckOutcome::Acknowledged, Some(row))) => Json(row).into_response(),
        Ok((AckOutcome::AlreadyAcknowledged, _)) => error(
            StatusCode::CONFLICT,
            format!(
                "alarm {id} is already acknowledged; send replace_note: true to change the note"
            ),
        ),
        Ok(_) => error(StatusCode::NOT_FOUND, format!("no alarm with id {id}")),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
