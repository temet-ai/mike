use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::persistence::launch::LinkOutcome;

pub(crate) const HELLO_PATH: &str = "/api/launch-ledger";
pub(crate) const RUNS_PATH: &str = "/api/launch-ledger/runs";
pub(crate) const LAUNCHES_PATH: &str = "/api/launch-ledger/launches";
pub(crate) const SESSIONS_PATH: &str = "/api/launch-ledger/sessions";
pub(crate) const LAUNCH_SETTING_PREFIX: &str = "launch_";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LedgerHello {
    pub(crate) database: bool,
    pub(crate) launch_brief: bool,
    pub(crate) settings: Vec<(String, Value)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LaunchId {
    pub(crate) id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RunQuery {
    pub(crate) run: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClosedBody {
    pub(crate) reason: String,
    pub(crate) process_exited: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChannelPresent {
    pub(crate) present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LinkBody {
    pub(crate) cse_id: String,
    pub(crate) cwd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LinkReply {
    pub(crate) outcome: LinkOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TurnQuery {
    pub(crate) since: DateTime<Utc>,
    pub(crate) prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TurnSeen {
    pub(crate) seen: bool,
}

pub(crate) fn launch_settings(rows: Vec<(String, Value)>) -> Vec<(String, Value)> {
    rows.into_iter()
        .filter(|(key, _)| key.starts_with(LAUNCH_SETTING_PREFIX))
        .collect()
}
