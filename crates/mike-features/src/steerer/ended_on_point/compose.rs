use super::questions::{CLAIMS_DONE, ENDS_ON};
use crate::llm::decisions::point::Asked;
use crate::steerer::guard::gates::{EndedOn, PhraseGates};

pub const ENDS_ON_AT: f64 = 0.5;
pub const DONE_AT: f64 = 0.5;
pub const JEV_MATCH: &str = "jev";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Report,
    Question,
    Handback,
    Promise,
}

impl Ending {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "report" => Some(Self::Report),
            "question" => Some(Self::Question),
            "handback" => Some(Self::Handback),
            "promise" => Some(Self::Promise),
            _ => None,
        }
    }

    pub fn ended_on(self) -> EndedOn {
        match self {
            Self::Report => EndedOn::Report,
            Self::Question => EndedOn::Question,
            Self::Handback => EndedOn::Handback,
            Self::Promise => EndedOn::PromiseOrWait,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub ending: Ending,
    pub ending_probability: f64,
    pub claims_done: bool,
    pub done_probability: f64,
}

impl Reading {
    pub fn score(&self) -> f64 {
        let done = if self.claims_done {
            self.done_probability
        } else {
            1.0 - self.done_probability
        };
        (self.ending_probability + done) / 2.0
    }
}

pub fn compose(asked: &Asked) -> Option<Reading> {
    if asked.failed() {
        return None;
    }
    let (key, ending_probability) = asked.choice(ENDS_ON)?;
    let done_probability = asked.noul(CLAIMS_DONE)?;
    Some(Reading {
        ending: Ending::parse(key)?,
        ending_probability,
        claims_done: done_probability >= DONE_AT,
        done_probability,
    })
}

pub fn apply(mut phrase: PhraseGates, reading: &Reading) -> PhraseGates {
    let ended = reading.ending.ended_on();
    phrase.ended_on = ended;
    phrase.g2 = ended == EndedOn::PromiseOrWait;
    phrase.g3 = matches!(ended, EndedOn::Handback | EndedOn::Question);
    phrase.g4 = reading.claims_done;
    if phrase.g2 && phrase.wait_match.is_none() {
        phrase.wait_match = Some(JEV_MATCH.to_owned());
    }
    if phrase.g3 && phrase.handback_match.is_none() {
        phrase.handback_match = Some(JEV_MATCH.to_owned());
    }
    if phrase.g4 && phrase.done_match.is_none() {
        phrase.done_match = Some(JEV_MATCH.to_owned());
    }
    phrase
}
