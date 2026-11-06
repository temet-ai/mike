use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::config::AppState;
use crate::db_cost::DbCostReport;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DbCostAnswer {
    pub(crate) monitored: bool,
    pub(crate) report: Option<DbCostReport>,
}

pub(crate) fn answer(monitored: bool, report: Option<&DbCostReport>) -> DbCostAnswer {
    DbCostAnswer {
        monitored,
        report: report.cloned(),
    }
}

pub(crate) async fn get_db_cost(State(state): State<AppState>) -> Json<DbCostAnswer> {
    let report = crate::db_cost::current();
    Json(answer(state.persist().is_some(), report.as_deref()))
}

#[cfg(test)]
mod tests;
