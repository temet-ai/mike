use serde_json::{Map, Value, json};

use super::questions::key;
use crate::dossier::entries::directives::extract;
use crate::llm::decisions::text::head_chars;
use crate::store::persistence::dossier_entries::DirectiveSourceRow;

const SENTENCE_CHARS: usize = 400;
const TURN_CHARS: usize = 800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub turn_id: i64,
    pub session_id: i64,
    pub request_id: i64,
    pub class: &'static str,
    pub sentence: String,
    pub turn_text: String,
}

pub fn sentence_around(text: &str, span: (usize, usize)) -> String {
    let chars: Vec<char> = text.chars().collect();
    let ends = |c: &char| matches!(c, '.' | '!' | '?' | '\n');
    let from = span.0.min(chars.len());
    let to = span.1.min(chars.len()).max(from);
    let start = chars[..from].iter().rposition(ends).map_or(0, |i| i + 1);
    let end = chars[to..]
        .iter()
        .position(ends)
        .map_or(chars.len(), |i| to + i + 1);
    chars[start..end]
        .iter()
        .collect::<String>()
        .trim()
        .to_owned()
}

pub fn units(sources: &[DirectiveSourceRow]) -> Vec<Unit> {
    let mut out = Vec::new();
    for source in sources {
        let mut seen: Vec<&'static str> = Vec::new();
        for found in extract(&source.user_prompt) {
            if seen.contains(&found.class) {
                continue;
            }
            seen.push(found.class);
            out.push(Unit {
                turn_id: source.turn_id,
                session_id: source.session_id,
                request_id: source.start_request_id,
                class: found.class,
                sentence: sentence_around(&source.user_prompt, found.span),
                turn_text: source.user_prompt.clone(),
            });
        }
    }
    out
}

pub fn state(group: &[&Unit]) -> Value {
    let units: Map<String, Value> = group
        .iter()
        .enumerate()
        .map(|(i, unit)| {
            (
                key(i),
                json!({
                    "action_class": unit.class,
                    "sentence": head_chars(&unit.sentence, SENTENCE_CHARS),
                    "turn": head_chars(&unit.turn_text, TURN_CHARS),
                }),
            )
        })
        .collect();
    json!({ "units": units })
}
