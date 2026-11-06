use super::{DispositionClass, PromptClass};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnDisposition {
    OperatorTurn,
    MachineSegment,
    Attach,
    Ignore,
}

pub struct LaneFacts<'a> {
    pub turn_origin: &'a str,
    pub is_user_prompt_type: bool,
    pub has_agent_id: bool,
}

fn is_primary_lane(turn_origin: &str) -> bool {
    turn_origin.eq_ignore_ascii_case("primary") || turn_origin.eq_ignore_ascii_case("primary_first")
}

pub fn turn_disposition(lane: &LaneFacts<'_>, class: &PromptClass) -> TurnDisposition {
    if !is_primary_lane(lane.turn_origin) || !lane.is_user_prompt_type || lane.has_agent_id {
        return TurnDisposition::Attach;
    }
    match class.kind.class() {
        DispositionClass::Operator => TurnDisposition::OperatorTurn,
        DispositionClass::InLaneMachine => TurnDisposition::MachineSegment,
        DispositionClass::LaneForeign => TurnDisposition::Attach,
        DispositionClass::Probe => TurnDisposition::Ignore,
    }
}
