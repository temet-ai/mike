use chrono::{DateTime, Utc};
use serde::Serialize;

use super::report::{DbCostReport, ReportStatus};
use crate::web_base::capability::{CapabilityRow, CapabilityState};

const ID: &str = "db_cost";
const LABEL: &str = "Database cost";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DbCostHealth {
    pub(crate) status: ReportStatus,
    pub(crate) generated_at: DateTime<Utc>,
    pub(crate) window_secs: Option<f64>,
    pub(crate) db_secs_per_hour: f64,
    pub(crate) statements_per_request: Option<f64>,
    pub(crate) offender_count: usize,
    pub(crate) detail: String,
    pub(crate) fix: String,
}

pub(crate) fn health_section(report: Option<&DbCostReport>) -> Option<DbCostHealth> {
    report.map(|report| DbCostHealth {
        status: report.status,
        generated_at: report.generated_at,
        window_secs: report.window_secs,
        db_secs_per_hour: report.db_secs_per_hour,
        statements_per_request: report.statements_per_request,
        offender_count: report.offenders.len(),
        detail: report.detail.clone(),
        fix: report.fix.clone(),
    })
}

pub(crate) fn apply_alarm(status: &'static str, report: Option<&DbCostReport>) -> &'static str {
    if report.is_some_and(|r| r.status == ReportStatus::Degraded) {
        "degraded"
    } else {
        status
    }
}

pub(crate) fn capability_row(monitored: bool, report: Option<&DbCostReport>) -> CapabilityRow {
    let (state, detail, fix) = match report {
        None if !monitored => (
            CapabilityState::Off,
            "Persistence is off, so there is no database cost to measure".to_owned(),
            String::new(),
        ),
        None => (
            CapabilityState::Active,
            "The database cost check has not taken its first snapshot yet".to_owned(),
            String::new(),
        ),
        Some(report) => (
            state_for(report.status),
            report.detail.clone(),
            report.fix.clone(),
        ),
    };
    CapabilityRow {
        id: ID,
        label: LABEL,
        state,
        detail,
        fix,
    }
}

fn state_for(status: ReportStatus) -> CapabilityState {
    match status {
        ReportStatus::Ok | ReportStatus::WarmingUp => CapabilityState::Active,
        ReportStatus::Degraded => CapabilityState::Degraded,
        ReportStatus::Unavailable => CapabilityState::Off,
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod health_tests;
