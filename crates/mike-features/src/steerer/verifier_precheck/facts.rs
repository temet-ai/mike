use serde_json::{Value, json};

use crate::llm::decisions::text::head_chars;
use crate::steerer::verdict::SteererVerdict;
use crate::wire::genuine::PromptKind;
use crate::wire::genuine::templates::match_template;
use crate::wire::machine_text::{MachineTextKind, classify};

const JUDGED_ASK_CHARS: usize = 800;
const MIN_QUOTE_CHARS: usize = 12;
const MAILBOX_MARK: &str = "unread cross-session mailbox message";
const COMPACTION_STARTS: [&str; 3] = [
    "<analysis>",
    "<summary>",
    "This session is being continued from a previous",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeSource {
    Verifier,
    Criteria,
    Mailbox,
    Other,
}

impl NudgeSource {
    pub fn code(self) -> &'static str {
        match self {
            Self::Verifier => "verifier",
            Self::Criteria => "criteria",
            Self::Mailbox => "mailbox",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskSource {
    Operator,
    Compaction,
    PeerMail,
    Brief,
    Unknown,
}

impl AskSource {
    pub fn code(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Compaction => "compaction",
            Self::PeerMail => "peer_mail",
            Self::Brief => "brief",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub nudge_source: NudgeSource,
    pub judged_ask: String,
    pub ask_source: AskSource,
    pub recent_commits: Vec<String>,
}

pub struct Sources<'a> {
    pub nudge: &'a str,
    pub verdict: &'a SteererVerdict,
    pub criteria: &'a [&'a str],
    pub accumulated_ask: &'a str,
    pub operator_ask: &'a str,
    pub last_user_message: &'a str,
}

impl Facts {
    pub fn derive(sources: &Sources<'_>, recent_commits: Vec<String>) -> Self {
        let judged_ask = judged_ask(sources).to_owned();
        Self {
            nudge_source: nudge_source(sources.nudge, sources.verdict, sources.criteria),
            ask_source: ask_source(&judged_ask),
            judged_ask,
            recent_commits,
        }
    }

    pub fn json(&self) -> Value {
        json!({
            "nudge_source": self.nudge_source.code(),
            "judged_ask": head_chars(&self.judged_ask, JUDGED_ASK_CHARS),
            "ask_source": self.ask_source.code(),
            "recent_commits": self.recent_commits,
        })
    }
}

fn judged_ask<'a>(sources: &Sources<'a>) -> &'a str {
    [
        sources.accumulated_ask,
        sources.operator_ask,
        sources.last_user_message,
    ]
    .into_iter()
    .find(|text| !text.trim().is_empty())
    .unwrap_or("")
}

pub fn nudge_source(nudge: &str, verdict: &SteererVerdict, criteria: &[&str]) -> NudgeSource {
    if nudge.trim().is_empty() {
        return NudgeSource::Other;
    }
    if nudge.trim() != verdict.nudge.trim() {
        return if nudge.contains(MAILBOX_MARK) {
            NudgeSource::Mailbox
        } else {
            NudgeSource::Other
        };
    }
    if quotes_a_criterion(nudge, criteria) {
        NudgeSource::Criteria
    } else {
        NudgeSource::Verifier
    }
}

fn quotes_a_criterion(nudge: &str, criteria: &[&str]) -> bool {
    let nudge = nudge.to_lowercase();
    criteria
        .iter()
        .map(|c| c.trim().to_lowercase())
        .filter(|c| c.chars().count() >= MIN_QUOTE_CHARS)
        .any(|c| nudge.contains(&c))
}

fn without_kind_tag(text: &str) -> &str {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix('[') else {
        return trimmed;
    };
    match rest.split_once("] ") {
        Some((tag, body)) if !tag.contains(char::is_whitespace) => body.trim_start(),
        _ => trimmed,
    }
}

pub fn ask_source(judged_ask: &str) -> AskSource {
    let text = without_kind_tag(judged_ask);
    if text.is_empty() {
        return AskSource::Unknown;
    }
    if COMPACTION_STARTS.iter().any(|m| text.starts_with(m)) {
        return AskSource::Compaction;
    }
    if let Some(kind) = classify(text) {
        return machine_source(kind);
    }
    match match_template(text) {
        None => AskSource::Operator,
        Some(PromptKind::PeerMessage) => AskSource::PeerMail,
        Some(PromptKind::LoopWakeup) => AskSource::Brief,
        Some(PromptKind::Compaction) => AskSource::Compaction,
        Some(_) => AskSource::Unknown,
    }
}

fn machine_source(kind: MachineTextKind) -> AskSource {
    match kind {
        MachineTextKind::CompactionRequest => AskSource::Compaction,
        MachineTextKind::MailboxDelivery
        | MachineTextKind::ChannelWake
        | MachineTextKind::TeammateMessage => AskSource::PeerMail,
        MachineTextKind::WakeCyclePrompt => AskSource::Brief,
        _ => AskSource::Unknown,
    }
}
