pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use serde_json::{Value, json};

use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
pub use questions::{ABOUT_AT, OPEN_AT, POINT, QUESTION_SET};
pub use state::NoteInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Rule,
    Ask,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gated {
    pub about: f64,
    pub said_open: f64,
    pub send: bool,
}

impl Gated {
    fn json(&self, kind: NoteKind) -> Value {
        json!({
            "kind": match kind { NoteKind::Rule => "rule", NoteKind::Ask => "ask" },
            "about": self.about,
            "said_open": self.said_open,
            "send": self.send,
            "about_at": ABOUT_AT,
            "open_at": OPEN_AT,
        })
    }
}

pub fn compose(asked: &Asked, kind: NoteKind) -> Option<Gated> {
    if asked.failed() {
        return None;
    }
    let about = asked.noul(questions::ABOUT)?;
    let said_open = asked.noul(questions::SAID_OPEN)?;
    let open_holds = kind == NoteKind::Ask && said_open >= OPEN_AT;
    Some(Gated {
        about,
        said_open,
        send: about >= ABOUT_AT && !open_holds,
    })
}

pub async fn admits(
    decider: Option<&Decider>,
    input: &NoteInput<'_>,
    subject: DecisionSubject,
) -> bool {
    let Some(decider) = decider else {
        return true;
    };
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(input),
            questions::questions(),
            subject,
            json!({ "about_at": ABOUT_AT, "open_at": OPEN_AT }),
        )
        .await;
    let gated = compose(&asked, input.kind);
    let send = !decider.applies() || gated.as_ref().is_none_or(|g| g.send);
    let verdict = gated.map_or(json!({ "send": true }), |g| g.json(input.kind));
    decider
        .mark(
            &asked,
            asked.composed(verdict),
            if send { "sent" } else { "held" },
        )
        .await;
    send
}
