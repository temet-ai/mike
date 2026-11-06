use crate::extract::request::TurnOrigin;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PromptClassKey {
    Main,
    Subagent(String),
}

impl PromptClassKey {
    pub(crate) fn resolve(turn_origin: &TurnOrigin, persona: Option<&str>) -> Option<Self> {
        match (turn_origin, persona) {
            (TurnOrigin::PrimaryFirst, _) => Some(Self::Main),
            (TurnOrigin::Subagent, Some(p)) => Some(Self::Subagent(p.to_string())),
            _ => None,
        }
    }

    pub(crate) fn lane(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Subagent(_) => "subagent",
        }
    }

    pub(crate) fn from_lane_and_agent_name(lane: &str, agent_name: Option<&str>) -> Option<Self> {
        match lane {
            "main" => Some(Self::Main),
            "subagent" => agent_name.map(|name| Self::Subagent(name.to_string())),
            _ => None,
        }
    }

    pub(crate) fn from_turn_origin_str(turn_origin: &str, persona: Option<&str>) -> Option<Self> {
        match (turn_origin, persona) {
            ("primary_first", _) => Some(Self::Main),
            ("subagent", Some(p)) => Some(Self::Subagent(p.to_string())),
            _ => None,
        }
    }
}
