use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AsOfMode {
    Knowledge,
    Reconstructed,
}

impl AsOfMode {
    pub fn code(self) -> &'static str {
        match self {
            AsOfMode::Knowledge => "knowledge",
            AsOfMode::Reconstructed => "reconstructed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AsOf {
    pub at: DateTime<Utc>,
    pub mode: AsOfMode,
}

impl AsOf {
    pub fn knowledge(at: DateTime<Utc>) -> Self {
        Self {
            at,
            mode: AsOfMode::Knowledge,
        }
    }

    pub fn reconstructed_for_eval(at: DateTime<Utc>) -> Self {
        Self {
            at,
            mode: AsOfMode::Reconstructed,
        }
    }
}
