pub mod claude;
pub mod codex;
pub mod codex_queue;
pub mod deliver;
pub mod envelope;
pub mod mailbox_link;
pub mod store;
pub mod store_sql;
pub mod worker;

use async_trait::async_trait;
use uuid::Uuid;

pub use codex_queue::observe_local_codex;
pub use worker::spawn_mail_outbox;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Claude,
    Codex,
}

impl Runtime {
    #[cfg(test)]
    pub fn as_str(self) -> &'static str {
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
pub enum Stage {
    Queued,
    Accepted,
    Consumed,
    ActionStarted,
    Completed,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
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
pub enum Reach {
    Idle,
    Busy { turn_id: String },
    Unreachable(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sighting {
    pub turn_id: Option<String>,
    pub stages: Vec<(Stage, String)>,
}

impl Sighting {
    pub fn has(&self, stage: Stage) -> bool {
        self.stages.iter().any(|(s, _)| *s == stage)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Seen(Sighting),
    NotSeen,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
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
pub struct Delivery<'a> {
    pub message: Uuid,
    pub attempt: Uuid,
    pub text: &'a str,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    Accepted { turn_id: String },
    TurnEnded(String),
    Refused(String),
    FailedBeforeSend(String),
    FailedAfterSend(String),
}

#[async_trait]
pub trait Recipient: Send + Sync {
    async fn generation(&self) -> Option<i64>;
    async fn reach(&self) -> Reach;
    async fn find(&self, message: Uuid) -> Found;
    async fn send(&self, delivery: &Delivery<'_>) -> Sent;
    async fn taken(&self, _message: Uuid, _consumed: bool) {}
}

#[cfg(test)]
pub mod codex_tests;
#[cfg(test)]
pub mod deliver_tests;
#[cfg(test)]
pub mod live_tests;
#[cfg(test)]
pub mod recovery_tests;
#[cfg(test)]
pub mod scripted;
