use super::disposition::{LaneFacts, TurnDisposition, turn_disposition};
use super::{PromptClass, PromptKind};

fn class(kind: PromptKind) -> PromptClass {
    PromptClass { kind, rule: None }
}

fn primary(kind: PromptKind) -> TurnDisposition {
    turn_disposition(
        &LaneFacts {
            turn_origin: "primary",
            is_user_prompt_type: true,
            has_agent_id: false,
        },
        &class(kind),
    )
}

#[test]
fn non_primary_lanes_attach() {
    for origin in [
        "subagent",
        "workflow_subagent",
        "title_sidecar",
        "next_input_suggestion_sidecar",
    ] {
        let lane = LaneFacts {
            turn_origin: origin,
            is_user_prompt_type: true,
            has_agent_id: false,
        };
        assert_eq!(
            turn_disposition(&lane, &class(PromptKind::Genuine)),
            TurnDisposition::Attach
        );
    }
}

#[test]
fn primary_tool_loop_and_continuation_attach() {
    let lane = LaneFacts {
        turn_origin: "primary_first",
        is_user_prompt_type: false,
        has_agent_id: false,
    };
    assert_eq!(
        turn_disposition(&lane, &class(PromptKind::Genuine)),
        TurnDisposition::Attach
    );
}

#[test]
fn turn_disposition_checks_agent_id_before_class() {
    let lane = LaneFacts {
        turn_origin: "primary",
        is_user_prompt_type: true,
        has_agent_id: true,
    };
    assert_eq!(
        turn_disposition(&lane, &class(PromptKind::PeerMessage)),
        TurnDisposition::Attach
    );
    assert_eq!(
        turn_disposition(&lane, &class(PromptKind::Genuine)),
        TurnDisposition::Attach
    );
}

#[test]
fn primary_user_prompt_follows_the_class() {
    assert_eq!(primary(PromptKind::Genuine), TurnDisposition::OperatorTurn);
    assert_eq!(
        primary(PromptKind::LoopWakeup),
        TurnDisposition::MachineSegment
    );
    assert_eq!(
        primary(PromptKind::HookFeedback),
        TurnDisposition::MachineSegment
    );
    assert_eq!(primary(PromptKind::Empty), TurnDisposition::MachineSegment);
    assert_eq!(primary(PromptKind::AgentLineage), TurnDisposition::Attach);
    assert_eq!(primary(PromptKind::ActivityLabel), TurnDisposition::Ignore);
    assert_eq!(primary(PromptKind::SessionRecap), TurnDisposition::Ignore);
}
