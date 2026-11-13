use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::persistence::launch::LinkOutcome;

pub const HELLO_PATH: &str = "/api/launch-ledger";
pub const RUNS_PATH: &str = "/api/launch-ledger/runs";
pub const LAUNCHES_PATH: &str = "/api/launch-ledger/launches";
pub const SESSIONS_PATH: &str = "/api/launch-ledger/sessions";
pub const LAUNCH_SETTING_PREFIX: &str = "launch_";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerHello {
    pub database: bool,
    pub launch_brief: bool,
    pub settings: Vec<(String, Value)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchId {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunQuery {
    pub run: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosedBody {
    pub reason: String,
    pub process_exited: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelPresent {
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkBody {
    pub cse_id: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkReply {
    pub outcome: LinkOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnQuery {
    pub since: DateTime<Utc>,
    pub prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnSeen {
    pub seen: bool,
}

pub fn launch_settings(rows: Vec<(String, Value)>) -> Vec<(String, Value)> {
    rows.into_iter()
        .filter(|(key, _)| key.starts_with(LAUNCH_SETTING_PREFIX))
        .collect()
}
