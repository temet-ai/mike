mod claude;
mod codex;
mod deliver;
mod envelope;
mod mailbox_link;
mod store;
mod store_sql;
mod worker;

use async_trait::async_trait;
use uuid::Uuid;

pub(crate) use worker::spawn_mail_outbox;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Runtime {
    Claude,
    Codex,
}

impl Runtime {
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Stage {
    Queued,
    Accepted,
    Consumed,
    ActionStarted,
    Completed,
}

impl Stage {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Accepted => "accepted",
            Self::Consumed => "consumed",
            Self::ActionStarted => "action_started",
            Self::Completed => "completed",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        [
            Self::Queued,
            Self::Accepted,
            Self::Consumed,
            Self::ActionStarted,
            Self::Completed,
        ]
        .into_iter()
        .find(|stage| stage.as_str() == raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    Idle,
    Busy { turn_id: String },
    Unreachable(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Sighting {
    pub(crate) turn_id: Option<String>,
    pub(crate) stages: Vec<(Stage, String)>,
}

impl Sighting {
    pub(crate) fn has(&self, stage: Stage) -> bool {
        self.stages.iter().any(|(s, _)| *s == stage)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Found {
    Seen(Sighting),
    NotSeen,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mode {
    Start,
    Steer { expected_turn: String },
}

impl Mode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Steer { .. } => "steer",
        }
    }

    fn expected_turn(&self) -> Option<&str> {
        match self {
            Self::Start => None,
            Self::Steer { expected_turn } => Some(expected_turn),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Delivery<'a> {
    pub(crate) message: Uuid,
    pub(crate) attempt: Uuid,
    pub(crate) text: &'a str,
    pub(crate) mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Sent {
    Accepted { turn_id: String },
    TurnEnded(String),
    Refused(String),
    FailedBeforeSend(String),
    FailedAfterSend(String),
}

#[async_trait]
pub(crate) trait Recipient: Send + Sync {
    async fn generation(&self) -> Option<i64>;
    async fn reach(&self) -> Reach;
    async fn find(&self, message: Uuid) -> Found;
    async fn send(&self, delivery: &Delivery<'_>) -> Sent;
    async fn taken(&self, _message: Uuid, _consumed: bool) {}
}

#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod deliver_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod scripted;
