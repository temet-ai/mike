use serde_json::{Value, json};

use crate::dossier::steering_facts::SteeringFacts;
use crate::steerer::guard::Question;

pub const ASK_CHARS: usize = 1500;
pub const FINAL_CHARS: usize = 2500;

pub struct PrecheckInput<'a> {
    pub question: Question,
    pub steerer_reason: &'a str,
    pub operator_ask: &'a str,
    pub final_message: &'a str,
    pub facts: Option<&'a SteeringFacts>,
}

pub fn tail(text: &str, chars: usize) -> &str {
    let count = text.chars().count();
    if count <= chars {
        return text;
    }
    let skip = count - chars;
    text.char_indices()
        .nth(skip)
        .map_or(text, |(i, _)| &text[i..])
}

fn concern(question: Question) -> &'static str {
    match question {
        Question::UnobservedDone => {
            "The assistant reports work as done, but nothing observed the result after its last change."
        }
        Question::UnbackedWait => {
            "The assistant says it is waiting on something, but nothing it started is running."
        }
        Question::Relapse => {
            "The assistant has slipped back into a behaviour it was corrected for earlier."
        }
        Question::AuthorisedHandback => {
            "The assistant asks permission for something it was already authorised to do."
        }
    }
}

fn facts_state(facts: Option<&SteeringFacts>) -> Value {
    let Some(f) = facts else {
        return json!({});
    };
    let o = &f.observation;
    let changed: Vec<Value> = o
        .changed
        .iter()
        .map(|c| {
            json!({
                "path": c.path,
                "kind": c.kind.label(),
                "lines_added": c.added,
                "lines_removed": c.removed,
                "committed": c.committed,
            })
        })
        .collect();
    json!({
        "changed_files": changed,
        "observed_after_last_change": o.observed_after_change,
        "last_observation_kind": o.observation_kind,
        "wait_backing": f.wait_backing.code(),
        "fleet_seat": f.session_class.as_ref().is_some_and(|c| c.seat.is_some()),
    })
}

pub fn state(input: &PrecheckInput<'_>) -> Value {
    json!({
        "operator_ask": tail(input.operator_ask, ASK_CHARS),
        "final_message": tail(input.final_message, FINAL_CHARS),
        "facts": facts_state(input.facts),
        "proposed_note": {
            "concern": concern(input.question),
            "why": input.steerer_reason,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_the_end_on_a_char_boundary() {
        assert_eq!(tail("héllo wörld", 5), "wörld");
        assert_eq!(tail("abc", 10), "abc");
    }

    #[test]
    fn state_without_facts_still_has_the_ask_and_the_message() {
        let s = state(&PrecheckInput {
            question: Question::UnobservedDone,
            steerer_reason: "no test after the edit",
            operator_ask: "fix the card",
            final_message: "done",
            facts: None,
        });
        assert_eq!(s["operator_ask"], "fix the card");
        assert_eq!(s["final_message"], "done");
        assert_eq!(s["facts"], json!({}));
        assert_eq!(s["proposed_note"]["why"], "no test after the edit");
    }
}
