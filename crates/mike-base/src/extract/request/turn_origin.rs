use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOrigin {
    Primary,
    PrimaryFirst,
    TitleSidecar,
    Subagent,
    AdvisorSidecar,
    QuotaProbe,
    MonitorSidecar,
    CountTokensSidecar,
    WebFetchSidecar,
    WorkflowSubagent,
    NextInputSuggestionSidecar,
}

impl fmt::Display for TurnOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primary => write!(f, "primary"),
            Self::PrimaryFirst => write!(f, "primary_first"),
            Self::TitleSidecar => write!(f, "title_sidecar"),
            Self::Subagent => write!(f, "subagent"),
            Self::AdvisorSidecar => write!(f, "advisor_sidecar"),
            Self::QuotaProbe => write!(f, "quota_probe"),
            Self::MonitorSidecar => write!(f, "monitor_sidecar"),
            Self::CountTokensSidecar => write!(f, "count_tokens_sidecar"),
            Self::WebFetchSidecar => write!(f, "web_fetch_sidecar"),
            Self::WorkflowSubagent => write!(f, "workflow_subagent"),
            Self::NextInputSuggestionSidecar => write!(f, "next_input_suggestion_sidecar"),
        }
    }
}

impl TurnOrigin {
    pub fn stamps_capture_identity(&self) -> bool {
        match self {
            Self::Primary | Self::PrimaryFirst | Self::Subagent | Self::WorkflowSubagent => true,
            Self::TitleSidecar
            | Self::AdvisorSidecar
            | Self::QuotaProbe
            | Self::MonitorSidecar
            | Self::CountTokensSidecar
            | Self::WebFetchSidecar
            | Self::NextInputSuggestionSidecar => false,
        }
    }

    pub fn resends_the_session_tool_list(&self) -> bool {
        match self {
            Self::Primary
            | Self::PrimaryFirst
            | Self::Subagent
            | Self::WorkflowSubagent
            | Self::CountTokensSidecar
            | Self::NextInputSuggestionSidecar => true,
            Self::TitleSidecar
            | Self::AdvisorSidecar
            | Self::QuotaProbe
            | Self::MonitorSidecar
            | Self::WebFetchSidecar => false,
        }
    }
}

#[cfg(test)]
#[path = "turn_origin_tests.rs"]
mod tests;
