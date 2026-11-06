use std::sync::Arc;

use arc_swap::ArcSwapOption;

pub(crate) mod budget;
pub(crate) mod health;
pub(crate) mod read;
pub(crate) mod report;
pub(crate) mod snapshot;
pub(crate) mod worker;

pub(crate) use report::DbCostReport;
pub(crate) use worker::spawn_db_cost_monitor;

static REPORT: ArcSwapOption<DbCostReport> = ArcSwapOption::const_empty();

pub(crate) fn current() -> Option<Arc<DbCostReport>> {
    REPORT.load_full()
}

pub(crate) fn publish(report: DbCostReport) {
    REPORT.store(Some(Arc::new(report)));
}
