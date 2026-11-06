use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    JustifyingShortcuts,
    DeflectingBlame,
    AvoidingWork,
    FakingUnderstanding,
    Overselling,
}

impl DiagnosticCategory {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::JustifyingShortcuts => "justifying_shortcuts",
            Self::DeflectingBlame => "deflecting_blame",
            Self::AvoidingWork => "avoiding_work",
            Self::FakingUnderstanding => "faking_understanding",
            Self::Overselling => "overselling",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub(crate) category: DiagnosticCategory,
    pub(crate) analyzer: String,
    pub(crate) advice: String,
    pub(crate) matched_text: String,
    pub(crate) match_range: (usize, usize),
    pub(crate) other_hits: usize,
    pub(crate) created_at: DateTime<Utc>,
}
