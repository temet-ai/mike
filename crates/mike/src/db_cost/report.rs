use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::budget::{StatementCost, by_cost_desc, statement_cost};
use super::snapshot::Window;
use crate::config::db_cost::DbCostConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportStatus {
    Ok,
    Degraded,
    WarmingUp,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DbCostReport {
    pub(crate) status: ReportStatus,
    pub(crate) maintenance:
        Option<crate::store::persistence::db_maintenance_reads::DatabaseMaintenance>,
    pub(crate) generated_at: DateTime<Utc>,
    pub(crate) window_from: Option<DateTime<Utc>>,
    pub(crate) window_to: Option<DateTime<Utc>>,
    pub(crate) window_secs: Option<f64>,
    pub(crate) stats_reset: Option<DateTime<Utc>>,
    pub(crate) statements_active: usize,
    pub(crate) total_calls: i64,
    pub(crate) db_secs: f64,
    pub(crate) db_secs_per_hour: f64,
    pub(crate) total_over_alarm: bool,
    pub(crate) proxied_requests: Option<i64>,
    pub(crate) statements_per_request: Option<f64>,
    pub(crate) offenders: Vec<StatementCost>,
    pub(crate) top_by_db_time: Vec<StatementCost>,
    pub(crate) thresholds: DbCostConfig,
    pub(crate) detail: String,
    pub(crate) fix: String,
}

pub(crate) const MISSING_EXTENSION_FIX: &str = "Enable pg_stat_statements in the capture database with scripts/capture-db/up.sh (it takes effect after one container restart)";

pub(crate) const DEGRADED_FIX: &str = "Open /api/db-cost for every offending statement with its full text, run EXPLAIN (ANALYZE, BUFFERS, IO) on the worst one and fix its plan, its poll rate or its cache; the budgets are the [db_cost] section of config.toml";

fn empty(
    status: ReportStatus,
    now: DateTime<Utc>,
    config: &DbCostConfig,
    detail: String,
    fix: &str,
) -> DbCostReport {
    DbCostReport {
        status,
        maintenance: None,
        generated_at: now,
        window_from: None,
        window_to: None,
        window_secs: None,
        stats_reset: None,
        statements_active: 0,
        total_calls: 0,
        db_secs: 0.0,
        db_secs_per_hour: 0.0,
        total_over_alarm: false,
        proxied_requests: None,
        statements_per_request: None,
        offenders: Vec::new(),
        top_by_db_time: Vec::new(),
        thresholds: config.clone(),
        detail,
        fix: fix.to_owned(),
    }
}

pub(crate) fn missing_extension(now: DateTime<Utc>, config: &DbCostConfig) -> DbCostReport {
    empty(
        ReportStatus::Unavailable,
        now,
        config,
        "pg_stat_statements is not installed in this database, so mike's DB cost is not measured"
            .to_owned(),
        MISSING_EXTENSION_FIX,
    )
}

pub(crate) fn read_failed(
    now: DateTime<Utc>,
    config: &DbCostConfig,
    error: &impl std::fmt::Display,
) -> DbCostReport {
    empty(
        ReportStatus::Unavailable,
        now,
        config,
        format!("reading pg_stat_statements failed: {error}"),
        "Check that pg_stat_statements is in shared_preload_libraries and that mike's role may read it (pg_read_all_stats)",
    )
}

pub(crate) fn warming_up(now: DateTime<Utc>, config: &DbCostConfig) -> DbCostReport {
    empty(
        ReportStatus::WarmingUp,
        now,
        config,
        format!(
            "first pg_stat_statements snapshot taken; the first window closes in {} s",
            config.interval_secs
        ),
        "",
    )
}

pub(crate) fn build(
    window: &Window,
    proxied_requests: Option<i64>,
    config: &DbCostConfig,
    now: DateTime<Utc>,
) -> DbCostReport {
    let window_secs = window.secs();
    let mut costs: Vec<StatementCost> = window
        .deltas
        .iter()
        .map(|(queryid, delta)| statement_cost(*queryid, *delta, window_secs, config))
        .collect();
    costs.sort_by(by_cost_desc);
    let total_calls: i64 = costs.iter().map(|cost| cost.calls).sum();
    let db_secs: f64 = costs.iter().map(|cost| cost.db_secs).sum();
    let db_secs_per_hour = db_secs * 3600.0 / window_secs;
    let total_over_alarm = db_secs_per_hour > config.total_alarm_db_secs_per_hour;
    let offenders: Vec<StatementCost> = costs.iter().filter(|c| c.offends()).cloned().collect();
    let status = if total_over_alarm || !offenders.is_empty() {
        ReportStatus::Degraded
    } else {
        ReportStatus::Ok
    };
    let statements_per_request = proxied_requests
        .filter(|count| *count > 0)
        .map(|count| total_calls as f64 / count as f64);
    let mut report = DbCostReport {
        status,
        maintenance: None,
        generated_at: now,
        window_from: Some(window.from),
        window_to: Some(window.to),
        window_secs: Some(window_secs),
        stats_reset: window.stats_reset,
        statements_active: costs.len(),
        total_calls,
        db_secs,
        db_secs_per_hour,
        total_over_alarm,
        proxied_requests,
        statements_per_request,
        offenders,
        top_by_db_time: costs.into_iter().take(config.top_statements).collect(),
        thresholds: config.clone(),
        detail: String::new(),
        fix: String::new(),
    };
    report.detail = describe(&report);
    if status == ReportStatus::Degraded {
        report.fix = DEGRADED_FIX.to_owned();
    }
    report
}

fn describe_statement(cost: &StatementCost) -> String {
    format!(
        "statement {} ran {} times at {:.1} ms mean ({} budget {:.0} ms, {:.2} calls/s) for {:.0} DB-seconds per hour",
        cost.queryid,
        cost.calls,
        cost.mean_ms,
        cost.tier.name(),
        cost.budget_ms,
        cost.calls_per_sec,
        cost.db_secs_per_hour
    )
}

pub(crate) fn describe(report: &DbCostReport) -> String {
    let minutes = report.window_secs.unwrap_or(0.0) / 60.0;
    let per_request = report
        .statements_per_request
        .map(|n| format!("; {n:.0} statements per proxied request"))
        .unwrap_or_default();
    let total = format!(
        "mike spent {:.0} DB-seconds per hour over the last {minutes:.0} min (alarm {:.0}){per_request}",
        report.db_secs_per_hour, report.thresholds.total_alarm_db_secs_per_hour
    );
    let worst = report.offenders.first().or(report.top_by_db_time.first());
    match (report.offenders.len(), worst) {
        (0, Some(top)) => format!("{total}; costliest: {}", describe_statement(top)),
        (0, None) => total,
        (count, Some(top)) => format!(
            "{count} statement(s) over budget; {total}; worst: {}",
            describe_statement(top)
        ),
        (count, None) => format!("{count} statement(s) over budget; {total}"),
    }
}

pub(crate) fn wanted_texts(report: &DbCostReport) -> Vec<i64> {
    let mut ids: Vec<i64> = report
        .offenders
        .iter()
        .chain(report.top_by_db_time.iter())
        .filter_map(StatementCost::queryid_number)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub(crate) fn attach_texts(report: &mut DbCostReport, texts: &HashMap<i64, String>) {
    for cost in report
        .offenders
        .iter_mut()
        .chain(report.top_by_db_time.iter_mut())
    {
        cost.query = cost.queryid_number().and_then(|id| texts.get(&id)).cloned();
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod report_tests;
