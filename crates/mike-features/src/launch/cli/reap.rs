use std::time::Duration;

use serde::Serialize;

use crate::launch::error::LaunchError;
use crate::launch::reap::{self, ClaudeProcess, SystemRunner};

const KILL_ESCALATE_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReapRow {
    pub pid: u32,
    pub host: String,
    pub session: Option<String>,
    pub model: Option<String>,
    pub uptime: String,
    pub cwd: String,
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
pub struct KilledRow {
    pub pid: u32,
    pub killed: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReapReport {
    pub to_kill: Vec<ReapRow>,
    pub skipped_self: Vec<ReapRow>,
    pub killed: Vec<KilledRow>,
}

impl ReapReport {
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!("{{\"to_kill\":[],\"skipped_self\":[],\"killed\":[],\"error\":\"json encoding failed: {e}\"}}")
        })
    }
}

pub async fn execute(yes: bool) -> Result<ReapReport, LaunchError> {
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
