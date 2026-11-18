use std::future::Future;
use std::path::PathBuf;

use serde_json::Value;

use super::perf::Perf;

#[derive(Debug, Clone)]
pub(crate) struct RepairJob {
    pub(crate) repair_id: Option<i64>,
    pub(crate) problem_id: Option<i64>,
    pub(crate) finding_id: Option<i64>,
    pub(crate) worktree: PathBuf,
    pub(crate) agent: &'static str,
    pub(crate) brief: String,
    pub(crate) inputs: Vec<(String, Value)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RepairRun {
    pub(crate) not_started: Option<String>,
    pub(crate) failure: Option<String>,
    pub(crate) perf: Perf,
}

pub(crate) trait RepairEngineer: Send + Sync {
    fn run_in_worktree(&self, job: &RepairJob) -> impl Future<Output = RepairRun> + Send;
}

pub(crate) trait LiveBuild: Send + Sync {
    fn commit(&self) -> Option<String>;
}

pub(crate) struct RunningBinary;

impl LiveBuild for RunningBinary {
    fn commit(&self) -> Option<String> {
        let sha = mike_base::change_log::current_build().sha;
        let full = sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit());
        full.then(|| sha.to_owned())
    }
}
