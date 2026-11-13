use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub enum RequestStatus {
    Pending,
    Streaming,
    Complete,
    Error(String),
}

impl RequestStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::Error(_))
    }
}

impl std::fmt::Display for RequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Streaming => write!(f, "Streaming"),
            Self::Complete => write!(f, "Complete"),
            Self::Error(e) => write!(f, "Error: {e}"),
        }
    }
}
