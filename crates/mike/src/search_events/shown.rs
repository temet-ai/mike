use serde_json::{Value, json};

use crate::api::handlers::SimilarTurnDto;

pub(super) const SHOWN_RESULTS: usize = 10;
pub(super) const SHOWN_CHARS: usize = 1_500;
const TURN_PART_SEPARATOR: &str = "\n\n";

fn cut(text: &str) -> String {
    text.chars().take(SHOWN_CHARS).collect()
}

pub(super) fn set_from_items(returned: &mut Value, delivered_items: &Value) {
    let Some(entries) = returned.as_array_mut() else {
        return;
    };
    let delivered = delivered_items.as_array().map(Vec::as_slice).unwrap_or(&[]);
    for (position, entry) in entries.iter_mut().enumerate() {
        if let Some(entry) = entry.as_object_mut() {
            entry.remove("shown");
        }
        let item = delivered
            .get(position)
            .filter(|_| position < SHOWN_RESULTS)
            .filter(|item| item["reference"] == entry["reference"]);
        let excerpt = item
            .and_then(|item| item["excerpt"].as_str())
            .filter(|excerpt| !excerpt.trim().is_empty());
        if let Some(excerpt) = excerpt {
            entry["shown"] = json!(cut(excerpt));
        }
    }
}

pub(super) fn set_first_turn(returned: &mut Value, turns: &[SimilarTurnDto]) {
    let Some(turn) = turns.first() else {
        return;
    };
    let text = format!(
        "{}{TURN_PART_SEPARATOR}{}",
        turn.turn.user_prompt, turn.turn.final_assistant_text
    );
    if text.trim().is_empty() {
        return;
    }
    if let Some(entry) = returned
        .as_array_mut()
        .and_then(|entries| entries.first_mut())
    {
        entry["shown"] = json!(cut(&text));
    }
}
