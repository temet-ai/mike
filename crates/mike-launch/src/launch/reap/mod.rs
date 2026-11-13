pub mod ancestors;
pub mod classify;
pub mod kill;
pub mod runner;
pub mod scan;

pub use ancestors::ReapPlan;
pub use kill::kill;
pub use runner::{ProcRunner, SystemRunner};
pub use scan::ClaudeProcess;

use crate::launch::error::LaunchError;

pub const MAX_ANCESTOR_HOPS: usize = 12;

pub async fn plan(runner: &dyn ProcRunner, self_pid: u32) -> Result<ReapPlan, LaunchError> {
    let procs = scan::scan(runner).await?;
    let anc = ancestors::ancestors(runner, self_pid).await;
    Ok(ancestors::reap_plan(procs, self_pid, &anc))
}
