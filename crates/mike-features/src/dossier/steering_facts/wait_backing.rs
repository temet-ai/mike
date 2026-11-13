use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

pub const REGISTRY_PARSES_SCHEDULED_LAUNCHES: bool = false;
pub const PEER_REPLY_WINDOW_HOURS: i64 = 2;
pub const PEER_ACTIVITY_WINDOW_MINUTES: i64 = 30;
pub const WAKE_SLACK_MINUTES: i64 = 5;
pub const UNKNOWN_WINDOW_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WaitBacking {
    WorkItem,
    ScheduledWake,
    Monitor,
    PeerReply,
    OperatorRequested,
    Unknown,
    None,
}

impl WaitBacking {
    pub fn code(self) -> &'static str {
        match self {
            Self::WorkItem => "work_item",
            Self::ScheduledWake => "scheduled_wake",
            Self::Monitor => "monitor",
            Self::PeerReply => "peer_reply",
            Self::OperatorRequested => "operator_requested",
            Self::Unknown => "unknown",
            Self::None => "none",
        }
    }

    pub fn from_source(source: &str) -> Option<Self> {
        match source {
            "work_item" => Some(Self::WorkItem),
            "scheduled_wake" => Some(Self::ScheduledWake),
            "monitor" => Some(Self::Monitor),
            "peer_reply" => Some(Self::PeerReply),
            "operator_requested" => Some(Self::OperatorRequested),
            "unregistered_launch" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct BackingRef {
    pub source: String,
    pub id: Option<i64>,
    pub kind: Option<String>,
    pub state: Option<String>,
    pub launched_at: Option<DateTime<Utc>>,
    pub due_at: Option<DateTime<Utc>>,
    pub owner_agent_id: Option<String>,
    pub peer_session_uuid: Option<String>,
    pub message_uuid: Option<String>,
    pub directive_id: Option<i64>,
}

pub fn peer_window_start(as_of: DateTime<Utc>) -> DateTime<Utc> {
    as_of - Duration::hours(PEER_REPLY_WINDOW_HOURS)
}

pub fn decide(mut refs: Vec<BackingRef>) -> (WaitBacking, Vec<BackingRef>) {
    refs.sort();
    let strongest = refs
        .iter()
        .filter_map(|r| WaitBacking::from_source(&r.source))
        .min();
    (strongest.unwrap_or(WaitBacking::None), refs)
}
