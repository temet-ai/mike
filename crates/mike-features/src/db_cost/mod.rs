use std::sync::Arc;

use arc_swap::ArcSwapOption;

pub mod budget;
pub mod health;
pub mod read;
pub mod report;
pub mod snapshot;
pub mod worker;

pub use report::DbCostReport;
pub use worker::spawn_db_cost_monitor;

static REPORT: ArcSwapOption<DbCostReport> = ArcSwapOption::const_empty();

pub fn current() -> Option<Arc<DbCostReport>> {
    REPORT.load_full()
}

pub fn publish(report: DbCostReport) {
    REPORT.store(Some(Arc::new(report)));
}
