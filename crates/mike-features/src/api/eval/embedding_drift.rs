use crate::store::persistence::eval_repo::EvalPersist as _;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use axum::Json;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;

use crate::api::semantic::{persist_error, unavailable};
use crate::base_state::BaseState;

const MAX_LIMIT: i64 = 500;

#[derive(Debug, Deserialize, Default)]
pub struct DriftQuery {
    pub stream: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

pub async fn get_embedding_drift(
    State(state): State<BaseState>,
    Query(q): Query<DriftQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let rows = match persist
        .knowledge()
        .list_drift_rows(q.stream.as_deref(), q.from, q.to)
        .await
    {
        Ok(rows) => rows,
        Err(e) => return persist_error(&e),
    };
    let alarms = persist
        .eval()
        .list_eval_alarms(&crate::store::persistence::eval_metrics::AlarmsFilter {
            state: crate::store::persistence::eval_metrics::AlarmState::All,
            kind: Some("embedding_drift".to_owned()),
            limit: MAX_LIMIT,
            ..Default::default()
        })
        .await;
    match alarms {
        Ok((alarms, alarm_total)) => {
            Json(json!({ "days": rows, "alarms": alarms, "alarm_total": alarm_total }))
                .into_response()
        }
        Err(e) => persist_error(&e),
    }
}
