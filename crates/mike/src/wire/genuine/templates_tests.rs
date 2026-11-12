use super::*;

fn operator(text: &str) -> PromptClass {
    classify_prompt(&PromptInput {
        text,
        last_user_message: None,
        author: "operator",
        has_agent_id: false,
    })
}

fn live(text: &str, last_user_message: &str, author: &str, has_agent_id: bool) -> PromptClass {
    classify_prompt(&PromptInput {
        text,
        last_user_message: Some(last_user_message),
        author,
        has_agent_id,
    })
}

const POSITIVES: [(&str, PromptKind); 27] = [
    (
        "[SUGGESTION MODE: Suggest what the user might naturally type next into Claude Code.]",
        PromptKind::AnalysisProbe,
    ),
    (
        "Another Claude session sent a message:\nhi",
        PromptKind::PeerMessage,
    ),
    (
        "<teammate-message teammate_id=\"a\">x</teammate-message>",
        PromptKind::PeerMessage,
    ),
    (
        "<agent-message from=\"b\">x</agent-message>",
        PromptKind::PeerMessage,
    ),
    ("[3 prior /loop wakeups found]", PromptKind::LoopWakeup),
    ("[1 prior /loop wakeup found]", PromptKind::LoopWakeup),
    (
        "<channel source=\"wake\" reason=\"x\">go</channel>",
        PromptKind::ChannelWake,
    ),
    ("# Autonomous loop check\nstep 1", PromptKind::LoopWakeup),
    ("call ScheduleWakeup now", PromptKind::LoopWakeup),
    ("<<autonomous-loop-dynamic>>", PromptKind::LoopWakeup),
    ("please Re-Arm the timer", PromptKind::LoopWakeup),
    (
        "Fleet coordinator wake: run the next cycle",
        PromptKind::LoopWakeup,
    ),
    ("observer sweep of the roster", PromptKind::LoopWakeup),
    ("w3b poll cycle now", PromptKind::LoopWakeup),
    ("check your mailbox and continue", PromptKind::LoopWakeup),
    ("Check inbox", PromptKind::LoopWakeup),
    ("continue as w2b", PromptKind::LoopWakeup),
    ("resume the fleet run", PromptKind::LoopWakeup),
    ("you are the observer for this run", PromptKind::LoopWakeup),
    ("jc21 fleet seat ready", PromptKind::LoopWakeup),
    ("rt-77 seat check", PromptKind::LoopWakeup),
    (
        "<task-notification>done</task-notification>",
        PromptKind::TaskNotification,
    ),
    (
        "[SYSTEM NOTIFICATION - NOT USER INPUT] task ended",
        PromptKind::BackgroundEvent,
    ),
    ("Tool loaded.\n", PromptKind::ToolLoaded),
    (
        "Stop hook feedback:\nack the mail",
        PromptKind::HookFeedback,
    ),
    (
        "This session is being continued from a previous conversation",
        PromptKind::Compaction,
    ),
    (
        "Summarize this portion of a Claude Code session transcript",
        PromptKind::AnalysisProbe,
    ),
];

const MORE_POSITIVES: [(&str, PromptKind); 7] = [
    (
        "Analyze this Claude Code session and extract facets",
        PromptKind::AnalysisProbe,
    ),
    (
        "Describe your most recent action in 3-5 words",
        PromptKind::ActivityLabel,
    ),
    (
        "has the following stopping condition been satisfied?",
        PromptKind::AnalysisProbe,
    ),
    ("<transcript>\nuser: hi", PromptKind::AnalysisProbe),
    (
        "The user stepped away and is coming back. Recap",
        PromptKind::SessionRecap,
    ),
    ("  Tool loaded.", PromptKind::ToolLoaded),
    (
        "say something. The user stepped away and is coming back",
        PromptKind::SessionRecap,
    ),
];

const NEGATIVES: [&str; 20] = [
    "please tell me what another Claude session sent",
    "the teammate-message tag is documented here",
    "we saw 3 prior loop wakeups found earlier",
    "the channel source=wake attribute matters",
    "the autonomous loop check is documented",
    "schedule wakeup later",
    "arm the device",
    "the fleet was fine; nothing more to say about weekly cycle",
    "please check the mailbox",
    "we could continue with the plan",
    "you are a helpful reviewer",
    "please add a seat to the table",
    "see the task-notification docs",
    "the SYSTEM NOTIFICATION docs",
    "Tool loaded. and then more text",
    "the stop hook feedback docs",
    "this session was continued",
    "summarize this portion please",
    "describe your most recent action",
    "add a retry to the http client",
];

#[test]
fn every_positive_template_matches_its_kind_with_rule_a() {
    for (text, kind) in POSITIVES.iter().chain(MORE_POSITIVES.iter()) {
        let class = operator(text);
        assert_eq!(class.kind, *kind, "text: {text}");
        assert_eq!(class.rule, Some(CaptureRule::A), "text: {text}");
    }
}

#[test]
fn every_negative_template_case_is_genuine() {
    for text in NEGATIVES {
        assert_eq!(operator(text).kind, PromptKind::Genuine, "text: {text}");
    }
}

#[test]
fn classify_prompt_fleet_observer_cycle_is_loop_wakeup() {
    assert_eq!(
        operator("observer: cycle 4 wake, read roster").kind,
        PromptKind::LoopWakeup
    );
}

#[test]
fn classify_prompt_re_arm_anywhere_is_loop_wakeup() {
    assert_eq!(
        operator("I think we should re-arm the deploy switch").kind,
        PromptKind::LoopWakeup
    );
}

#[test]
fn classify_prompt_teammate_message_before_re_arm_is_peer_message() {
    assert_eq!(
        operator("<teammate-message>please re-arm</teammate-message>").kind,
        PromptKind::PeerMessage
    );
}

#[test]
fn classify_prompt_request_interrupted_by_user_is_genuine() {
    assert_eq!(
        operator("[Request interrupted by user]").kind,
        PromptKind::Genuine
    );
}

#[test]
fn classify_prompt_tool_loaded_with_trailing_text_is_genuine() {
    assert_eq!(
        operator("Tool loaded.\nand then more").kind,
        PromptKind::Genuine
    );
}

#[test]
fn classify_prompt_empty_without_last_user_message_is_empty() {
    assert_eq!(operator("").kind, PromptKind::Empty);
    assert_eq!(operator("  \n").kind, PromptKind::Empty);
}

#[test]
fn classify_prompt_empty_with_system_notification_envelope_is_background_event() {
    let class = live(
        "",
        "<system-reminder>\n[SYSTEM NOTIFICATION - NOT USER INPUT]\nBackground task done.",
        "harness",
        false,
    );
    assert_eq!(class.kind, PromptKind::BackgroundEvent);
    assert_eq!(class.rule, Some(CaptureRule::Envelope));
}

#[test]
fn classify_prompt_empty_with_mailbox_envelope_is_mailbox_delivery() {
    let class = live(
        "",
        "<system-reminder>\nCross-session message from session abc\n</system-reminder>",
        "harness",
        false,
    );
    assert_eq!(class.kind, PromptKind::MailboxDelivery);
}

#[test]
fn classify_prompt_empty_with_plain_last_message_is_empty() {
    assert_eq!(
        live("", "hello there", "harness", false).kind,
        PromptKind::Empty
    );
}

#[test]
fn classify_prompt_task_notification_rewrite_is_task_notification_live_only() {
    let text = "Task notification: build finished";
    assert_eq!(
        live(text, "x", "harness", false).kind,
        PromptKind::TaskNotification
    );
    assert_eq!(operator(text).kind, PromptKind::Genuine);
}

#[test]
fn classify_prompt_agent_id_on_genuine_text_is_agent_lineage() {
    let class = live("do the work", "do the work", "agent", true);
    assert_eq!(class.kind, PromptKind::AgentLineage);
    assert_eq!(class.rule, Some(CaptureRule::C));
}

#[test]
fn classify_prompt_harness_author_on_genuine_text_is_harness_authored() {
    let class = live("do the work", "do the work", "harness", false);
    assert_eq!(class.kind, PromptKind::HarnessAuthored);
    assert_eq!(class.rule, Some(CaptureRule::C));
}

#[test]
fn classify_prompt_peer_message_with_agent_id_stays_peer_message() {
    let class = live(
        "<teammate-message>hi</teammate-message>",
        "x",
        "agent",
        true,
    );
    assert_eq!(class.kind, PromptKind::PeerMessage);
    assert_eq!(class.rule, Some(CaptureRule::A));
}

#[test]
fn is_genuine_text_matches_classify_prompt_for_operator_author() {
    for (text, _) in POSITIVES.iter().chain(MORE_POSITIVES.iter()) {
        assert!(!is_genuine_text(text), "text: {text}");
    }
    for text in NEGATIVES {
        assert!(is_genuine_text(text), "text: {text}");
    }
}

#[test]
fn stored_codes_round_trip_and_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for kind in PromptKind::ALL {
        assert!(seen.insert(kind.stored_code()));
        assert_eq!(PromptKind::from_stored_code(kind.stored_code()), Some(kind));
    }
    assert_eq!(PromptKind::from_stored_code("no-such-kind"), None);
}

#[test]
fn kind_classes_follow_the_vocabulary_table() {
    assert_eq!(PromptKind::Genuine.class(), DispositionClass::Operator);
    assert_eq!(
        PromptKind::LoopWakeup.class(),
        DispositionClass::InLaneMachine
    );
    assert_eq!(PromptKind::ActivityLabel.class(), DispositionClass::Probe);
    assert_eq!(
        PromptKind::AgentLineage.class(),
        DispositionClass::LaneForeign
    );
    assert_eq!(PromptKind::MailboxDelivery.j2_code(), "empty");
    assert_eq!(PromptKind::AgentLineage.j2_code(), "loop-wakeup");
}

#[test]
fn recorded_fleet_acceptance_wake_is_not_operator_feedback() {
    let wake = "Acceptance seat for fleet lab-thread-2 (b9d4723f-2c08-4641-9150-36e96a5020fe): read inbox with get_session_messages, judge any slice reports, and pass over the criteria C1-C7 for progress.";
    assert_eq!(operator(wake).kind, PromptKind::LoopWakeup);
    assert_eq!(operator(wake).kind.class(), DispositionClass::InLaneMachine);
    assert_eq!(
        operator(&format!("Investigate why this wake failed: {wake}")).kind,
        PromptKind::Genuine
    );
    assert_eq!(
        operator("Acceptance seat for fleet should check the screenshots").kind,
        PromptKind::Genuine
    );
}

#[test]
fn tool_written_openings_are_tool_prompts_not_operator_words() {
    let tool_written = [
        "A session-scoped Stop hook is now active with condition: \"done\"",
        "The tether plugin sent a message:\nMail from mike",
        "  The fleet-comms plugin sent a message:\nhi",
        "Perform a web search for the query: sqlx",
        "You score how well a stand-in predicted the answer",
        "Coordinator wake for fleet open-issues: act",
    ];
    for text in tool_written {
        let class = operator(text);
        assert_eq!(class.kind, PromptKind::ToolPrompt, "text: {text}");
        assert_eq!(class.kind.class(), DispositionClass::InLaneMachine);
        assert!(is_tool_prompt(text));
        assert!(!is_genuine_text(text));
    }
    assert_eq!(PromptKind::ToolPrompt.stored_code(), "tool-prompt");
    assert_eq!(PromptKind::ToolPrompt.j2_code(), "background-event");
    for text in [
        "why did the tether plugin sent a message: show up?",
        "Please perform a web search for the query: sqlx",
        "Coordinator wake for fleets is broken",
    ] {
        assert_eq!(operator(text).kind, PromptKind::Genuine, "text: {text}");
        assert!(!is_tool_prompt(text));
    }
}
