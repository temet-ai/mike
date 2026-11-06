use crate::dossier::genuine::markers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MachineTextKind {
    ChannelWake,
    TaskNotification,
    MailboxDelivery,
    SteppedAwayRecap,
    SteeringPayload,
    TeammateMessage,
    HookFeedback,
    SystemNotification,
    ToolLoadConfirmation,
    WakeCyclePrompt,
    CompactionRequest,
}

impl MachineTextKind {
    pub(crate) const fn in_words(self) -> &'static str {
        match self {
            Self::ChannelWake => "a message from another session or from mike",
            Self::TaskNotification => "the result of a background task",
            Self::MailboxDelivery => "mail from another session",
            Self::SteppedAwayRecap => "Claude Code's request for a recap",
            Self::SteeringPayload => "a steering note",
            Self::TeammateMessage => "a message from another agent",
            Self::HookFeedback => "feedback from a hook",
            Self::SystemNotification => "a system notification",
            Self::ToolLoadConfirmation => "a tool that finished loading",
            Self::WakeCyclePrompt => "a scheduled wake-up",
            Self::CompactionRequest => "Claude Code's request to summarise the conversation",
        }
    }
}

pub(crate) fn classify(prompt: &str) -> Option<MachineTextKind> {
    let trimmed = prompt.trim_start();
    if trimmed.starts_with(markers::CHANNEL_OPEN) {
        Some(MachineTextKind::ChannelWake)
    } else if prompt.contains(markers::TASK_NOTIFICATION) {
        Some(MachineTextKind::TaskNotification)
    } else if prompt.contains(markers::TEAMMATE_MESSAGE) || prompt.contains(markers::AGENT_MESSAGE)
    {
        Some(MachineTextKind::TeammateMessage)
    } else if prompt.contains(markers::STOP_HOOK_FEEDBACK) {
        Some(MachineTextKind::HookFeedback)
    } else if prompt.contains(markers::SYSTEM_NOTIFICATION) {
        Some(MachineTextKind::SystemNotification)
    } else if trimmed.starts_with(markers::TOOL_LOADED) {
        Some(MachineTextKind::ToolLoadConfirmation)
    } else if prompt.contains(markers::AUTONOMOUS_LOOP_DYNAMIC)
        || prompt.contains(markers::AUTONOMOUS_LOOP)
        || prompt.contains(markers::FLEET_CYCLE_WAKE)
    {
        Some(MachineTextKind::WakeCyclePrompt)
    } else if prompt.contains(markers::CROSS_SESSION_MESSAGE)
        || prompt.contains(markers::PLUGIN_PROMPT_NOTICE)
        || crate::store::persistence::mailbox::is_mail_text(prompt)
    {
        Some(MachineTextKind::MailboxDelivery)
    } else if prompt.contains(markers::STEPPED_AWAY) {
        Some(MachineTextKind::SteppedAwayRecap)
    } else if prompt.contains(markers::STEERING_NOTE) {
        Some(MachineTextKind::SteeringPayload)
    } else if prompt.contains(markers::COMPACTION_TEXT_ONLY)
        && prompt.contains(markers::COMPACTION_SUMMARY)
    {
        Some(MachineTextKind::CompactionRequest)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_channel_wake_envelope() {
        assert_eq!(
            classify(
                "<channel source=\"wake\" reason=\"steering\" pending=\"1\">check in</channel>"
            ),
            Some(MachineTextKind::ChannelWake)
        );
    }

    #[test]
    fn classify_task_notification_block() {
        let text = "<task-notification>\n<task-id>abc</task-id>\n<status>completed</status>\n</task-notification>\nany follow-up?";
        assert_eq!(classify(text), Some(MachineTextKind::TaskNotification));
    }

    #[test]
    fn classify_mailbox_delivery_envelope() {
        let text = "<system-reminder>\nCross-session message from session abc123, queued 2026-08-03T00:00:00Z. \
                     To reply, use the `send_session_message` tool with to=\"abc123\".\n\nbody\n</system-reminder>";
        assert_eq!(classify(text), Some(MachineTextKind::MailboxDelivery));
    }

    #[test]
    fn classify_stepped_away_recap() {
        let text = "The user stepped away and is coming back. Recap in under 40 words what you were doing.";
        assert_eq!(classify(text), Some(MachineTextKind::SteppedAwayRecap));
    }

    #[test]
    fn classify_steering_payload_echo() {
        let text = "<system-reminder>Steering note from the operator's proxy:\n\nfocus on X\n\nAct on it, or say briefly why it doesn't apply.</system-reminder>";
        assert_eq!(classify(text), Some(MachineTextKind::SteeringPayload));
    }

    #[test]
    fn classify_compaction_request_but_not_an_operator_who_asks_for_a_recap() {
        let compaction = "CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.\n\nYour task is to create a detailed summary of the conversation so far.";
        assert_eq!(
            classify(compaction),
            Some(MachineTextKind::CompactionRequest)
        );
        assert_eq!(
            classify("give me a detailed summary of the conversation with the vendor"),
            None
        );
    }

    #[test]
    fn every_kind_has_plain_words_for_the_judge() {
        assert_eq!(
            MachineTextKind::TaskNotification.in_words(),
            "the result of a background task"
        );
        assert!(
            MachineTextKind::CompactionRequest
                .in_words()
                .contains("summarise")
        );
    }

    #[test]
    fn classify_genuine_operator_ask_is_none() {
        assert_eq!(classify("please add a retry to the http client"), None);
    }

    #[test]
    fn classify_empty_prompt_is_none() {
        assert_eq!(classify(""), None);
    }

    #[test]
    fn classify_teammate_message_envelope_no_prefix() {
        let text = "<teammate-message teammate_id=\"team-lead\" summary=\"Verify #121 resolved-as-fixed claim\">\n\
                     Independent acceptance verification. Repo: /path (branch main).\n\
                     </teammate-message>";
        assert_eq!(classify(text), Some(MachineTextKind::TeammateMessage));
    }

    #[test]
    fn classify_teammate_message_envelope_with_relay_prefix() {
        let text = "Another Claude session sent a message:\n\
                     <teammate-message teammate_id=\"verify-121\" color=\"blue\" summary=\"Full #121 verdict\">\n\
                     Not blocked — I completed the full verification. No `lease_expires_at` filter anywhere.\n\
                     </teammate-message>\n\n\
                     This came from another Claude session — not typed by your user.";
        assert_eq!(classify(text), Some(MachineTextKind::TeammateMessage));
    }

    #[test]
    fn classify_stop_hook_feedback() {
        let text = "Stop hook feedback:\nyou have 1 unacknowledged cross-session message(s); \
                     handle them, then acknowledge: curl -X POST http://127.0.0.1:9989/api/sessions/x/messages/y/ack";
        assert_eq!(classify(text), Some(MachineTextKind::HookFeedback));
    }

    #[test]
    fn classify_wake_cycle_prompt_run_specific_specimen() {
        let text = "Fleet coordinator cycle wake (steerer-firing-model successor run): \
                     (1) heartbeat + inbox; (2) poll worker heads rust-a / rust-b / observer via get_session_head.";
        assert_eq!(classify(text), Some(MachineTextKind::WakeCyclePrompt));
    }

    #[test]
    fn classify_wake_cycle_autonomous_loop_sentinel() {
        assert_eq!(
            classify("<<autonomous-loop-dynamic>>"),
            Some(MachineTextKind::WakeCyclePrompt)
        );
        assert_eq!(
            classify("<<autonomous-loop>>"),
            Some(MachineTextKind::WakeCyclePrompt)
        );
    }

    #[test]
    fn classify_tool_load_confirmation() {
        assert_eq!(
            classify("Tool loaded."),
            Some(MachineTextKind::ToolLoadConfirmation)
        );
    }

    #[test]
    fn classify_system_notification_block() {
        let text = "[SYSTEM NOTIFICATION - NOT USER INPUT]\nBackground task xyz completed.";
        assert_eq!(classify(text), Some(MachineTextKind::SystemNotification));
    }

    #[test]
    fn classify_steering_payload_pushed_whole_message_matches_request_time_appended_shape() {
        let payload = "<system-reminder>Steering note from the operator's proxy:\n\nfocus on X\n\nAct on it, or say briefly why it doesn't apply.</system-reminder>";
        let pushed_whole_message = payload;
        let request_time_appended = format!(
            "can you also take a look at the flaky retry test while you're at it?\n\n{payload}"
        );

        assert_eq!(
            classify(pushed_whole_message),
            Some(MachineTextKind::SteeringPayload)
        );
        assert_eq!(
            classify(pushed_whole_message),
            classify(request_time_appended.as_str()),
            "a pushed whole-message payload and the same payload appended after existing \
             operator content must classify identically"
        );
    }

    #[test]
    fn classify_mailbox_delivery_pushed_whole_message_matches_request_time_appended_shape() {
        let payload = "<system-reminder>\nCross-session message from session abc123, queued 2026-08-03T00:00:00Z. \
                     To reply, use the `send_session_message` tool with to=\"abc123\".\n\nbody\n</system-reminder>";
        let pushed_whole_message = payload;
        let request_time_appended =
            format!("what's the status on the migration script?\n\n{payload}");

        assert_eq!(
            classify(pushed_whole_message),
            Some(MachineTextKind::MailboxDelivery)
        );
        assert_eq!(
            classify(pushed_whole_message),
            classify(request_time_appended.as_str()),
            "a pushed whole-message payload and the same payload appended after existing \
             operator content must classify identically"
        );
    }
}
