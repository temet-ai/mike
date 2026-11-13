use super::PromptKind;
use crate::wire::machine_text::{self, MachineTextKind};

pub fn classify_envelope(last_user_message: &str) -> Option<PromptKind> {
    machine_text::classify(last_user_message).map(|kind| match kind {
        MachineTextKind::ChannelWake => PromptKind::ChannelWake,
        MachineTextKind::TaskNotification => PromptKind::TaskNotification,
        MachineTextKind::MailboxDelivery => PromptKind::MailboxDelivery,
        MachineTextKind::SteppedAwayRecap => PromptKind::SessionRecap,
        MachineTextKind::SteeringPayload => PromptKind::SteeringPayload,
        MachineTextKind::TeammateMessage => PromptKind::PeerMessage,
        MachineTextKind::HookFeedback => PromptKind::HookFeedback,
        MachineTextKind::SystemNotification => PromptKind::BackgroundEvent,
        MachineTextKind::ToolLoadConfirmation => PromptKind::ToolLoaded,
        MachineTextKind::WakeCyclePrompt => PromptKind::LoopWakeup,
        MachineTextKind::CompactionRequest => PromptKind::Compaction,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn every_machine_text_kind_maps_to_its_prompt_kind() {
        let cases = [
            (
                "<channel source=\"wake\">x</channel>",
                PromptKind::ChannelWake,
            ),
            (
                "<task-notification>x</task-notification>",
                PromptKind::TaskNotification,
            ),
            (
                "Cross-session message from session abc",
                PromptKind::MailboxDelivery,
            ),
            (
                "The user stepped away and is coming back",
                PromptKind::SessionRecap,
            ),
            (
                "Steering note from the operator's proxy: x",
                PromptKind::SteeringPayload,
            ),
            (
                "<teammate-message id=\"a\">x</teammate-message>",
                PromptKind::PeerMessage,
            ),
            (
                "<agent-message from=\"a\">x</agent-message>",
                PromptKind::PeerMessage,
            ),
            ("Stop hook feedback:\nx", PromptKind::HookFeedback),
            (
                "[SYSTEM NOTIFICATION - NOT USER INPUT] x",
                PromptKind::BackgroundEvent,
            ),
            ("Tool loaded.", PromptKind::ToolLoaded),
            ("<<autonomous-loop-dynamic>>", PromptKind::LoopWakeup),
            ("Fleet coordinator cycle wake", PromptKind::LoopWakeup),
        ];
        for (text, kind) in cases {
            assert_eq!(classify_envelope(text), Some(kind), "text: {text}");
        }
    }

    #[test]
    fn plain_text_has_no_envelope() {
        assert_eq!(classify_envelope("please add a retry"), None);
    }
}
