use super::questions::ORIGIN;
use crate::llm::decisions::point::Asked;

pub const APPLY_AT: f64 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Operator,
    Mail,
    Hook,
    LoopWakeup,
    Probe,
}

impl Origin {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "operator" => Some(Self::Operator),
            "mail" => Some(Self::Mail),
            "hook" => Some(Self::Hook),
            "loop_wakeup" => Some(Self::LoopWakeup),
            "probe" => Some(Self::Probe),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Mail => "mail",
            Self::Hook => "hook",
            Self::LoopWakeup => "loop_wakeup",
            Self::Probe => "probe",
        }
    }

    pub fn kind_code(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Mail => "mailbox-delivery",
            Self::Hook => "hook-feedback",
            Self::LoopWakeup => "loop-wakeup",
            Self::Probe => "analysis-probe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    pub origin: Origin,
    pub probability: f64,
}

impl Verdict {
    pub fn applies(&self) -> bool {
        self.probability >= APPLY_AT
    }

    pub fn clears_repeat_key(&self) -> bool {
        self.origin == Origin::Operator
    }
}

pub fn compose(asked: &Asked) -> Option<Verdict> {
    if asked.failed() {
        return None;
    }
    let (key, probability) = asked.choice(ORIGIN)?;
    Some(Verdict {
        origin: Origin::parse(key)?,
        probability,
    })
}
