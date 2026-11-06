pub mod scanner;
pub(crate) mod store;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentSource {
    Observed,
    FileBacked,
    Dynamic,
    Builtin,
}

impl AgentSource {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::FileBacked => "file_backed",
            Self::Dynamic => "dynamic",
            Self::Builtin => "builtin",
        }
    }

    pub(crate) fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "observed" => Ok(Self::Observed),
            "file_backed" => Ok(Self::FileBacked),
            "dynamic" => Ok(Self::Dynamic),
            "builtin" => Ok(Self::Builtin),
            other => Err(format!("unknown agent source: {other:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    pub id: i64,
    pub(crate) name: String,
    pub(crate) definition_path: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) source: AgentSource,
    pub(crate) created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_source_round_trips_every_variant_including_builtin() {
        for source in [
            AgentSource::Observed,
            AgentSource::FileBacked,
            AgentSource::Dynamic,
            AgentSource::Builtin,
        ] {
            assert_eq!(AgentSource::from_str(source.as_str()), Ok(source));
        }
    }
}
