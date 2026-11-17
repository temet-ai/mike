use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

pub(super) const FILE_READ_STAGE: &str = "file_read_compression";
pub(super) const SHELL_STAGE: &str = "shell_output_compression";
pub(super) const FILE_SHARE: &str = "file_read_compression.forward_share";
pub(super) const MIN_SESSIONS_PER_ARM: usize = 40;
pub(super) const P_LIMIT: f64 = 0.1;
pub(super) const WIDE_SHARE: f64 = 90.0;
pub(super) const FOLLOWUP_WINDOW: f64 = 1800.0;
pub(super) const SOURCE_BATCH: usize = 500;
pub(super) const ARM_FORWARD: &str = "forward";
pub(super) const ARM_CONTROL: &str = "control";
pub(super) const NO_SHELL_ARMS: &str =
    "Shell-output compression: no shell-output arms were recorded in the window.";

pub(super) type Arms = BTreeMap<i64, String>;
pub(super) type Gaps = BTreeMap<String, u64>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct Lane {
    pub(super) session_id: i64,
    pub(super) agent_instance_id: Option<i64>,
    pub(super) claude_code_agent_id: Option<String>,
}

impl Lane {
    pub(super) fn has_agent(&self) -> bool {
        self.agent_instance_id.is_some_and(|id| id != 0)
            || self
                .claude_code_agent_id
                .as_deref()
                .is_some_and(|id| !id.is_empty())
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct Who {
    pub(super) session_id: i64,
    pub(super) agent_instance_id: Option<i64>,
    pub(super) claude_code_agent_id: Option<String>,
    pub(super) origin: String,
}

impl Who {
    pub(super) fn lane(&self) -> Option<Lane> {
        let lane = Lane {
            session_id: self.session_id,
            agent_instance_id: self.agent_instance_id,
            claude_code_agent_id: self.claude_code_agent_id.clone(),
        };
        let primary = matches!(self.origin.as_str(), "primary" | "primary_first");
        (lane.has_agent() || primary).then_some(lane)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct Cut {
    #[sqlx(flatten)]
    pub(super) who: Who,
    pub(super) created_at: DateTime<Utc>,
    pub(super) wire_id: String,
    pub(super) reason: String,
    pub(super) detail: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct Source {
    #[sqlx(flatten)]
    pub(super) who: Who,
    pub(super) id: i64,
    pub(super) wire_id: String,
    pub(super) file_path: Option<String>,
    pub(super) input: Option<Value>,
    pub(super) content: Value,
    pub(super) result_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct Call {
    #[sqlx(flatten)]
    pub(super) who: Who,
    pub(super) id: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) tool_name: String,
    pub(super) file_path: Option<String>,
    pub(super) input: Option<Value>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct UsageRow {
    #[sqlx(flatten)]
    pub(super) who: Who,
    pub(super) fresh: i64,
    pub(super) cache_write: i64,
    pub(super) cache_read: i64,
    pub(super) output: i64,
    pub(super) complete_usage: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Usage {
    pub(super) fresh: i64,
    pub(super) cache_write: i64,
    pub(super) cache_read: i64,
    pub(super) output: i64,
    pub(super) complete_usage: bool,
}

impl Usage {
    pub(super) fn empty() -> Self {
        Self {
            fresh: 0,
            cache_write: 0,
            cache_read: 0,
            output: 0,
            complete_usage: true,
        }
    }

    pub(super) fn add(&mut self, row: &UsageRow) {
        self.fresh += row.fresh;
        self.cache_write += row.cache_write;
        self.cache_read += row.cache_read;
        self.output += row.output;
        self.complete_usage = self.complete_usage && row.complete_usage.unwrap_or(false);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ExposureRow {
    pub(super) cuts: u64,
    pub(super) reread: u64,
    pub(super) edit_hidden: u64,
    pub(super) exposures_with_hidden_edit: u64,
    pub(super) exposures_with_reread: u64,
}

pub(super) type HarmTable = BTreeMap<(String, String), ExposureRow>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ShellRow {
    pub(super) cuts: u64,
    pub(super) needed_later: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ShellTable {
    pub(super) forward: ShellRow,
    pub(super) control: ShellRow,
    pub(super) by_command: BTreeMap<String, ShellRow>,
    pub(super) left_whole_now: ShellRow,
}

impl ShellTable {
    pub(super) fn row_mut(&mut self, arm: &str) -> Option<&mut ShellRow> {
        match arm {
            ARM_FORWARD => Some(&mut self.forward),
            ARM_CONTROL => Some(&mut self.control),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(super) struct Decision {
    pub(super) action: &'static str,
    pub(super) key: &'static str,
    pub(super) value: Value,
    pub(super) why: String,
}

pub(super) fn bump(gaps: &mut Gaps, name: &str) {
    *gaps.entry(name.to_string()).or_insert(0) += 1;
}

pub(super) fn stamp(at: DateTime<Utc>) -> f64 {
    at.timestamp_micros() as f64 / 1_000_000.0
}
