use std::time::Duration;

use serde::Serialize;

use crate::launch::error::LaunchError;
use crate::launch::reap::{self, ClaudeProcess, SystemRunner};

const KILL_ESCALATE_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReapRow {
    pub(crate) pid: u32,
    pub(crate) host: String,
    pub(crate) session: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) uptime: String,
    pub(crate) cwd: String,
}

impl From<&ClaudeProcess> for ReapRow {
    fn from(p: &ClaudeProcess) -> Self {
        Self {
            pid: p.pid,
            host: p.host.clone(),
            session: p.session.clone(),
            model: p.model.clone(),
            uptime: p.uptime.clone(),
            cwd: p.cwd.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct KilledRow {
    pub(crate) pid: u32,
    pub(crate) killed: bool,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReapReport {
    pub(crate) to_kill: Vec<ReapRow>,
    pub(crate) skipped_self: Vec<ReapRow>,
    pub(crate) killed: Vec<KilledRow>,
}

impl ReapReport {
    pub(crate) fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!("{{\"to_kill\":[],\"skipped_self\":[],\"killed\":[],\"error\":\"json encoding failed: {e}\"}}")
        })
    }
}

pub(crate) async fn execute(yes: bool) -> Result<ReapReport, LaunchError> {
    let runner = SystemRunner;
    let plan = reap::plan(&runner, std::process::id()).await?;
    let mut killed = Vec::new();
    if yes {
        for proc in &plan.to_kill {
            let outcome = reap::kill(&runner, proc.pid, KILL_ESCALATE_AFTER).await;
            killed.push(KilledRow {
                pid: proc.pid,
                killed: outcome.is_ok(),
                detail: outcome.err().map(|e| e.to_string()),
            });
        }
    }
    Ok(ReapReport {
        to_kill: plan.to_kill.iter().map(ReapRow::from).collect(),
        skipped_self: plan.skipped_self.iter().map(ReapRow::from).collect(),
        killed,
    })
}
