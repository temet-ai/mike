use serde_json::{Value, json};

use super::questions::USED_AS_WEASEL;
use crate::llm::decisions::point::Asked;
use crate::llm::decisions::text::tail_chars;

pub const WEASEL_AT: f64 = 0.5;
const CONTEXT_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    pub weasel: bool,
    pub probability: f64,
}

impl Verdict {
    pub fn label(&self) -> &'static str {
        if self.weasel { "weasel" } else { "not_weasel" }
    }

    pub fn score(&self) -> f64 {
        self.probability.max(1.0 - self.probability)
    }
}

pub fn state(word: &str, text: &str) -> Value {
    json!({
        "word": word,
        "text": tail_chars(text.trim_end(), CONTEXT_CHARS),
    })
}

pub fn compose(asked: &Asked) -> Option<Verdict> {
    if asked.failed() {
        return None;
    }
    let probability = asked.noul(USED_AS_WEASEL)?;
    Some(Verdict {
        weasel: probability >= WEASEL_AT,
        probability,
    })
}
