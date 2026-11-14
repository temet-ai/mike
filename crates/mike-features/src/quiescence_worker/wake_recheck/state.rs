use serde_json::{Value, json};

use crate::dossier::steering_facts::SteeringFacts;
use crate::llm::decisions::text::head_chars;

const NUDGE_CHARS: usize = 700;
const BACKING_LIMIT: usize = 5;

pub struct WakeInput<'a> {
    pub nudge: &'a str,
    pub question: Option<&'a str>,
    pub facts: Option<&'a SteeringFacts>,
}

pub fn state(input: &WakeInput<'_>) -> Value {
    let facts = input.facts.map(|f| {
        let backing: Vec<Value> = f
            .wait_backing_refs
            .iter()
            .take(BACKING_LIMIT)
            .map(|b| json!({ "source": b.source, "kind": b.kind, "state": b.state }))
            .collect();
        json!({
            "wait_backing_now": f.wait_backing.code(),
            "backing": backing,
            "changed_since_last_check": f.observation.observed_after_change.map(|checked| !checked),
            "last_observation_kind": f.observation.observation_kind,
            "paths_changed": f.observation.changed.len(),
            "change_summary": f.observation.change_summary(),
        })
    });
    json!({
        "nudge": head_chars(input.nudge, NUDGE_CHARS),
        "question": input.question,
        "facts": facts,
    })
}
