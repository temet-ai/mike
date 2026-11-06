use std::time::Duration;

use super::jobs::Job;
use crate::store::persistence::PersistHandle;

pub(super) async fn holds(persist: Option<&PersistHandle>, job: &Job, tick: Duration) -> bool {
    match persist {
        Some(persist) if job.leased => persist.holds_worker_role(job.name, tick).await,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::super::jobs::{AUDIT, PROMOTE};
    use super::*;

    #[tokio::test]
    async fn a_per_machine_job_always_holds_its_own_run() {
        assert!(holds(None, &PROMOTE, Duration::from_secs(60)).await);
    }

    #[tokio::test]
    async fn a_leased_job_without_a_database_runs_unleased() {
        assert!(holds(None, &AUDIT, Duration::from_secs(60)).await);
    }
}
