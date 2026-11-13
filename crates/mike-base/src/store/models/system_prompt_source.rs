use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub enum SystemPromptSource {
    Exact,
    Class,
    AgentInstance,
    SessionMain,
    #[default]
    NotCaptured,
}

impl SystemPromptSource {
    pub fn from_db_code(code: &str) -> Self {
        match code {
            "exact" => Self::Exact,
            "class" => Self::Class,
            "agent_instance" => Self::AgentInstance,
            "session_main" => Self::SessionMain,
            _ => Self::NotCaptured,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemPromptSource;

    #[test]
    fn from_db_code_known_codes_round_trip() {
        assert_eq!(
            SystemPromptSource::from_db_code("exact"),
            SystemPromptSource::Exact
        );
        assert_eq!(
            SystemPromptSource::from_db_code("class"),
            SystemPromptSource::Class
        );
        assert_eq!(
            SystemPromptSource::from_db_code("agent_instance"),
            SystemPromptSource::AgentInstance
        );
        assert_eq!(
            SystemPromptSource::from_db_code("session_main"),
            SystemPromptSource::SessionMain
        );
    }

    #[test]
    fn from_db_code_unknown_code_falls_back_to_not_captured() {
        assert_eq!(
            SystemPromptSource::from_db_code("not_captured"),
            SystemPromptSource::NotCaptured
        );
        assert_eq!(
            SystemPromptSource::from_db_code("bogus"),
            SystemPromptSource::NotCaptured
        );
    }

    #[test]
    fn default_is_not_captured() {
        assert_eq!(
            SystemPromptSource::default(),
            SystemPromptSource::NotCaptured
        );
    }

    #[test]
    fn serializes_as_snake_case() -> anyhow::Result<()> {
        let json = serde_json::to_string(&SystemPromptSource::AgentInstance)?;
        assert_eq!(json, "\"agent_instance\"");
        Ok(())
    }
}
