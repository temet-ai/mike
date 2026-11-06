pub mod disposition;
pub mod envelope;
pub mod markers;
pub mod repeat;
pub mod stream;
pub mod templates;
pub mod words;

pub use words::operator_words;

#[cfg(test)]
mod disposition_tests;
#[cfg(test)]
mod repeat_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod templates_tests;

pub const CLASSIFIER_VERSION: &str = "genuine-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptKind {
    Genuine,
    Empty,
    PeerMessage,
    LoopWakeup,
    ChannelWake,
    TaskNotification,
    BackgroundEvent,
    ToolLoaded,
    HookFeedback,
    Compaction,
    MailboxDelivery,
    SteeringPayload,
    ActivityLabel,
    AnalysisProbe,
    SessionRecap,
    HarnessAuthored,
    AgentLineage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispositionClass {
    Operator,
    InLaneMachine,
    LaneForeign,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureRule {
    A,
    C,
    Envelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveRule {
    A,
    B,
    AB,
    C,
    Envelope,
}

pub struct PromptInput<'a> {
    pub text: &'a str,
    pub last_user_message: Option<&'a str>,
    pub author: &'a str,
    pub has_agent_id: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptClass {
    pub kind: PromptKind,
    pub rule: Option<CaptureRule>,
}

impl PromptKind {
    pub const ALL: [PromptKind; 17] = [
        PromptKind::Genuine,
        PromptKind::Empty,
        PromptKind::PeerMessage,
        PromptKind::LoopWakeup,
        PromptKind::ChannelWake,
        PromptKind::TaskNotification,
        PromptKind::BackgroundEvent,
        PromptKind::ToolLoaded,
        PromptKind::HookFeedback,
        PromptKind::Compaction,
        PromptKind::MailboxDelivery,
        PromptKind::SteeringPayload,
        PromptKind::ActivityLabel,
        PromptKind::AnalysisProbe,
        PromptKind::SessionRecap,
        PromptKind::HarnessAuthored,
        PromptKind::AgentLineage,
    ];

    pub fn stored_code(self) -> &'static str {
        match self {
            PromptKind::Genuine => "operator",
            PromptKind::Empty => "empty-prompt",
            PromptKind::PeerMessage => "teammate-message",
            PromptKind::LoopWakeup => "loop-wakeup",
            PromptKind::ChannelWake => "channel-wake",
            PromptKind::TaskNotification => "task-notification",
            PromptKind::BackgroundEvent => "background-event",
            PromptKind::ToolLoaded => "tool-loaded",
            PromptKind::HookFeedback => "hook-feedback",
            PromptKind::Compaction => "compaction",
            PromptKind::MailboxDelivery => "mailbox-delivery",
            PromptKind::SteeringPayload => "steering-payload",
            PromptKind::ActivityLabel => "activity-label",
            PromptKind::AnalysisProbe => "analysis-probe",
            PromptKind::SessionRecap => "session-recap",
            PromptKind::HarnessAuthored => "harness-authored",
            PromptKind::AgentLineage => "agent-lineage",
        }
    }

    pub fn j2_code(self) -> &'static str {
        match self {
            PromptKind::Genuine => "genuine",
            PromptKind::Empty | PromptKind::MailboxDelivery | PromptKind::SteeringPayload => {
                "empty"
            }
            PromptKind::PeerMessage => "peer-message",
            PromptKind::LoopWakeup | PromptKind::HarnessAuthored | PromptKind::AgentLineage => {
                "loop-wakeup"
            }
            PromptKind::ChannelWake => "channel-wake",
            PromptKind::TaskNotification => "task-notification",
            PromptKind::BackgroundEvent => "background-event",
            PromptKind::ToolLoaded => "tool-loaded",
            PromptKind::HookFeedback => "hook-feedback",
            PromptKind::Compaction => "compaction",
            PromptKind::ActivityLabel | PromptKind::AnalysisProbe => "analysis-probe",
            PromptKind::SessionRecap => "session-recap",
        }
    }

    pub fn class(self) -> DispositionClass {
        match self {
            PromptKind::Genuine => DispositionClass::Operator,
            PromptKind::ActivityLabel | PromptKind::AnalysisProbe | PromptKind::SessionRecap => {
                DispositionClass::Probe
            }
            PromptKind::AgentLineage => DispositionClass::LaneForeign,
            _ => DispositionClass::InLaneMachine,
        }
    }

    pub fn from_stored_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.stored_code() == code)
    }
}

fn classify_blank(last_user_message: Option<&str>) -> PromptClass {
    let envelope = last_user_message
        .filter(|last| !last.trim().is_empty())
        .and_then(envelope::classify_envelope);
    match envelope {
        Some(kind) => PromptClass {
            kind,
            rule: Some(CaptureRule::Envelope),
        },
        None => PromptClass {
            kind: PromptKind::Empty,
            rule: None,
        },
    }
}

fn classify_lineage(input: &PromptInput<'_>) -> PromptClass {
    if input.has_agent_id {
        return PromptClass {
            kind: PromptKind::AgentLineage,
            rule: Some(CaptureRule::C),
        };
    }
    if input.author == "harness" {
        return PromptClass {
            kind: PromptKind::HarnessAuthored,
            rule: Some(CaptureRule::C),
        };
    }
    PromptClass {
        kind: PromptKind::Genuine,
        rule: None,
    }
}

pub fn classify_prompt(input: &PromptInput<'_>) -> PromptClass {
    if input.text.trim().is_empty() {
        return classify_blank(input.last_user_message);
    }
    if let Some(kind) = templates::match_template(input.text) {
        return PromptClass {
            kind,
            rule: Some(CaptureRule::A),
        };
    }
    if input.last_user_message.is_some() && templates::is_task_notification_rewrite(input.text) {
        return PromptClass {
            kind: PromptKind::TaskNotification,
            rule: Some(CaptureRule::A),
        };
    }
    classify_lineage(input)
}

pub fn is_genuine_text(text: &str) -> bool {
    classify_prompt(&PromptInput {
        text,
        last_user_message: None,
        author: "operator",
        has_agent_id: false,
    })
    .kind
        == PromptKind::Genuine
}

pub fn opened_by_something_else(opened_by: Option<&str>, prompt: Option<&str>) -> bool {
    opened_by.is_some_and(|by| by != "operator") || prompt.is_some_and(|p| !is_genuine_text(p))
}
