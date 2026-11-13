use crate::store::persistence::web_repo::WebPersist as _;
use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

use super::{bounded, forks, logs};
use crate::store::persistence::wright::{CheckRow, RecordedRun};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::wright::definition::parse_probe;
use crate::wright::{CheckOutcome, CheckRun, Finding, Probe};

#[derive(Debug, Clone)]
pub(crate) struct WatchContext {
    pub(crate) persist: PersistHandle,
    pub(crate) log_dir: Option<PathBuf>,
    pub(crate) machine: Option<Uuid>,
}

async fn run_probe(check: &CheckRow, ctx: &WatchContext) -> Result<Vec<Finding>, String> {
    let pool = ctx.persist.pool();
    match parse_probe(&check.probe)? {
        Probe::Sql => {
            let query = check
                .query
                .as_deref()
                .ok_or_else(|| format!("{}: a sql check has no query", check.name))?;
            bounded::findings(pool, &check.name, query, &check.params).await
        }
        Probe::Logs => logs::check(ctx.log_dir.as_deref(), Utc::now(), &check.params).await,
        Probe::ColdForks => forks::check(pool, &check.name, &check.params).await,
    }
}

pub(crate) async fn probe(check: &CheckRow, ctx: &WatchContext) -> CheckOutcome {
    match run_probe(check, ctx).await {
        Ok(findings) => CheckOutcome::Complete(findings),
        Err(reason) => CheckOutcome::Unknown(reason),
    }
}

pub(crate) async fn run_check(
    check: &CheckRow,
    ctx: &WatchContext,
) -> Result<(CheckOutcome, RecordedRun), PersistError> {
    let started_at = Utc::now();
    let outcome = probe(check, ctx).await;
    let recorded = ctx
        .persist
        .web()
        .record_wright_run(&CheckRun {
            check: &check.name,
            source: &check.source,
            cadence: &check.cadence,
            started_at,
            outcome: &outcome,
            machine: ctx.machine,
        })
        .await?;
    Ok((outcome, recorded))
}
