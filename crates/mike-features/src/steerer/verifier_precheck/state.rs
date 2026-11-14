use serde_json::{Value, json};

use crate::llm::decisions::text::{head_chars, tail_chars};
use crate::steerer::verdict::SteererVerdict;

const ASK_CHARS: usize = 800;
const END_CHARS: usize = 2_500;
const NUDGE_CHARS: usize = 600;
const CHECKLIST_ITEMS: usize = 8;

pub struct FireInput<'a> {
    pub operator_ask: &'a str,
    pub turn_end: &'a str,
    pub nudge: &'a str,
    pub verdict: &'a SteererVerdict,
}

pub fn state(input: &FireInput<'_>) -> Value {
    let checklist: Vec<Value> = input
        .verdict
        .checklist
        .iter()
        .take(CHECKLIST_ITEMS)
        .map(|item| json!({ "criterion": item.criterion, "status": item.status }))
        .collect();
    json!({
        "operator_ask": head_chars(input.operator_ask, ASK_CHARS),
        "turn_end": tail_chars(input.turn_end, END_CHARS),
        "nudge": head_chars(input.nudge, NUDGE_CHARS),
        "verifier_reason": head_chars(&input.verdict.reason, NUDGE_CHARS),
        "checklist": checklist,
    })
}
