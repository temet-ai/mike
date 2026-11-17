use serde::Deserialize;
use serde_json::{Value, json};

use crate::store::persistence::roster_side_call::{RecentTurn, RosterCandidate};

pub const SYSTEM_PROMPT: &str = "You keep the roster row of a Claude Code session: the line other \
sessions and the operator read to know what this session is doing. You get the row as it stands and \
the session's last few turns (the operator's message, then the assistant's final reply).\n\n\
Write:\n\
- name: 2 to 4 lowercase words joined by hyphens, naming the work (not the tool, not 'session').\n\
- goal: one plain sentence, the outcome the session is working toward. Keep the current goal \
unless the turns show the work has changed.\n\
- next_steps: one plain sentence, what the session does next, or 'Waiting for the operator' when \
the last reply hands back to the operator.\n\n\
Use the operator's own words for names of things. No jargon, no hedging, no quotes.";

const MAX_NAME: usize = 40;
const MAX_SENTENCE: usize = 280;

#[derive(Debug, Clone, Deserialize)]
pub struct RosterAnswer {
    pub name: String,
    pub goal: String,
    pub next_steps: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterFields {
    pub name: String,
    pub goal: String,
    pub next_steps: String,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "goal": {"type": "string"},
            "next_steps": {"type": "string"}
        },
        "required": ["name", "goal", "next_steps"],
        "additionalProperties": false
    })
}

pub fn digest(candidate: &RosterCandidate, turns: &[RecentTurn]) -> String {
    let mut out = String::from("Current row:\n");
    out.push_str(&format!(
        "name: {}\ngoal: {}\nnext_steps: {}\n",
        candidate.display_name.as_deref().unwrap_or(""),
        candidate.goal.as_deref().unwrap_or(""),
        candidate.next_steps.as_deref().unwrap_or(""),
    ));
    if let Some(title) = candidate.title.as_deref().filter(|t| !t.is_empty()) {
        out.push_str(&format!("Claude Code's title for the session: {title}\n"));
    }
    for (index, turn) in turns.iter().enumerate() {
        out.push_str(&format!("\nTurn {}:\n", index + 1));
        out.push_str(&format!(
            "Operator: {}\n",
            strip_reminders(&turn.user_prompt)
        ));
        out.push_str(&format!("Assistant: {}\n", turn.final_text.trim()));
        if !turn.files_touched.is_empty() {
            out.push_str(&format!("Files: {}\n", turn.files_touched.join(", ")));
        }
    }
    out
}

pub fn strip_reminders(text: &str) -> String {
    const OPEN: &str = "<system-reminder";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        match rest[start..].find(CLOSE) {
            Some(end) => rest = &rest[start + end + CLOSE.len()..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn clean(answer: RosterAnswer) -> Option<RosterFields> {
    let name = slug(&answer.name);
    let goal = sentence(&answer.goal);
    let next_steps = sentence(&answer.next_steps);
    if goal.is_empty() || next_steps.is_empty() {
        return None;
    }
    Some(RosterFields {
        name,
        goal,
        next_steps,
    })
}

fn slug(raw: &str) -> String {
    let lowered: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let joined = lowered
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    cut(&joined, MAX_NAME).trim_end_matches('-').to_owned()
}

fn sentence(raw: &str) -> String {
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    cut(flat.trim_matches(['"', '\'']), MAX_SENTENCE).to_owned()
}

fn cut(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_reminders_drops_reminder_blocks_and_keeps_the_ask() {
        let text = "<system-reminder>ctx\nmore</system-reminder>\nhold on, think <system-reminder x>a</system-reminder> carefully";
        assert_eq!(strip_reminders(text), "hold on, think carefully");
    }

    #[test]
    fn strip_reminders_unclosed_block_drops_the_rest() {
        assert_eq!(strip_reminders("ask <system-reminder>never closed"), "ask");
    }

    #[test]
    fn clean_slugs_the_name_and_flattens_sentences() {
        let fields = clean(RosterAnswer {
            name: "Side Call  Bookkeeping!".to_owned(),
            goal: " \"Let mike fill\n the roster\" ".to_owned(),
            next_steps: "Build slice one.".to_owned(),
        });
        assert_eq!(
            fields,
            Some(RosterFields {
                name: "side-call-bookkeeping".to_owned(),
                goal: "Let mike fill the roster".to_owned(),
                next_steps: "Build slice one.".to_owned(),
            })
        );
    }

    #[test]
    fn clean_blank_goal_is_refused() {
        let fields = clean(RosterAnswer {
            name: "x".to_owned(),
            goal: "  ".to_owned(),
            next_steps: "y".to_owned(),
        });
        assert!(fields.is_none());
    }
}
