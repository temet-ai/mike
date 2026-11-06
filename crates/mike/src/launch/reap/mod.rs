mod ancestors;
mod classify;
mod kill;
mod runner;
mod scan;

pub(crate) use ancestors::ReapPlan;
pub(crate) use kill::kill;
pub(crate) use runner::{ProcRunner, SystemRunner};
pub(crate) use scan::ClaudeProcess;

use crate::launch::error::LaunchError;

pub(crate) const MAX_ANCESTOR_HOPS: usize = 12;

pub(crate) async fn plan(runner: &dyn ProcRunner, self_pid: u32) -> Result<ReapPlan, LaunchError> {
    let procs = scan::scan(runner).await?;
    let anc = ancestors::ancestors(runner, self_pid).await;
    Ok(ancestors::reap_plan(procs, self_pid, &anc))
}
