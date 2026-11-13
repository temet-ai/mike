use chrono::{DateTime, Utc};

use super::allocate::{AllocSection, SectionBody};
use super::cut::{cut, cut_front, head_chars};
use crate::dossier::view::types::DossierInputs;
use crate::store::persistence::dossier_view::DossierTurnRow;
use crate::store::persistence::steering::SessionNote;
use crate::wire::genuine::{opened_by_something_else, operator_words};
use crate::wire::machine_text;

pub const EARLIER_NOTES_HEADING: &str = "NOTES THIS SESSION ALREADY RECEIVED (from any steerer, oldest first; do not raise a point one of these made, and send nothing about a point the session has answered):";

pub const TURN_OPENING_HEADING: &str = "WHAT OPENED THIS TURN:";

pub const EARLIER_TURNS_HEADING: &str = "EARLIER TURNS AND HOW EACH ENDED (oldest first, from the last turn the operator opened; an ask that one of these turns delivered is closed and is not open in this turn):";

const OPERATOR_OPENED: &str = "a message from the operator";

const TURNS_SHOWN_AFTER_THE_OPERATORS: usize = 3;
const OPENING_WORDS_SHOWN: usize = 240;
const ENDING_SHOWN: usize = 600;

fn fmt_time(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn how_it_was_taken(note: &SessionNote) -> String {
    let reaction = match note.reaction.as_deref() {
        Some("complied") => "the session acted on it",
        Some("declined") => "the session said it did not apply",
        Some("ignored") => "the session did not answer it",
        Some("no_reply") => "it never reached the session",
        _ => "the session's answer is not graded yet",
    };
    match note.outcome_label.as_deref() {
        Some("right") => format!("{reaction}; the note was judged right"),
        Some("wrong") => format!("{reaction}; the note was judged wrong"),
        _ => reaction.to_owned(),
    }
}

fn note_line(note: &SessionNote) -> String {
    format!(
        "- {} from {}: {}\n  ({})",
        fmt_time(note.created_at),
        note.steerer_name,
        note.payload_text.as_deref().unwrap_or("").trim(),
        how_it_was_taken(note)
    )
}

pub fn earlier_notes_section(inputs: &DossierInputs) -> AllocSection {
    AllocSection {
        id: "earlier_notes",
        heading: EARLIER_NOTES_HEADING.to_owned(),
        body: SectionBody::List {
            items: inputs.notes.iter().map(note_line).collect(),
            noun: "notes are",
        },
    }
}

fn opened_by_the_operator(turn: &DossierTurnRow) -> bool {
    !opened_by_something_else(turn.opened_by.as_deref(), Some(&turn.prompt))
}

fn what_else_opened(turn: &DossierTurnRow) -> &'static str {
    machine_text::classify(&turn.prompt)
        .map_or("something other than the operator", |kind| kind.in_words())
}

fn turns_closed_before(inputs: &DossierInputs, turn_id: Option<i64>) -> Vec<&DossierTurnRow> {
    let before = turn_id
        .and_then(|id| inputs.turns.iter().find(|turn| turn.turn_id == id))
        .map_or(i64::MAX, |turn| turn.start_request_id);
    let mut closed: Vec<&DossierTurnRow> = inputs
        .turns
        .iter()
        .filter(|turn| turn.closed_at.is_some() && turn.start_request_id < before)
        .collect();
    closed.sort_by_key(|turn| (turn.start_request_id, turn.turn_id));
    closed
}

fn from_the_operators_last_turn<'a>(closed: &[&'a DossierTurnRow]) -> Vec<&'a DossierTurnRow> {
    let newest = closed.len().saturating_sub(TURNS_SHOWN_AFTER_THE_OPERATORS);
    match closed.iter().rposition(|turn| opened_by_the_operator(turn)) {
        Some(asked) if asked < newest => {
            let mut shown = vec![closed[asked]];
            shown.extend_from_slice(&closed[newest..]);
            shown
        }
        Some(asked) => closed[asked..].to_vec(),
        None => closed[newest..].to_vec(),
    }
}

fn who_opened(turn: &DossierTurnRow) -> String {
    if opened_by_the_operator(turn) {
        format!(
            "the operator: \"{}\"",
            cut(operator_words(&turn.prompt).trim(), OPENING_WORDS_SHOWN)
        )
    } else {
        what_else_opened(turn).to_owned()
    }
}

fn how_it_ended(turn: &DossierTurnRow) -> String {
    let kept = turn
        .text_cut_at
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(usize::MAX);
    let text = head_chars(&turn.final_text, kept).trim();
    if text.is_empty() {
        "(it wrote no text)".to_owned()
    } else {
        cut_front(text, ENDING_SHOWN)
    }
}

fn earlier_turn(turn: &DossierTurnRow) -> String {
    format!(
        "turn {} at {}, opened by {}\nIt ended: {}",
        turn.turn_id,
        fmt_time(turn.created_at),
        who_opened(turn),
        how_it_ended(turn)
    )
}

pub fn earlier_turns_section(inputs: &DossierInputs, turn_id: Option<i64>) -> AllocSection {
    let closed = turns_closed_before(inputs, turn_id);
    AllocSection {
        id: "earlier_turns",
        heading: EARLIER_TURNS_HEADING.to_owned(),
        body: SectionBody::List {
            items: from_the_operators_last_turn(&closed)
                .into_iter()
                .map(earlier_turn)
                .collect(),
            noun: "turns are",
        },
    }
}

fn opening_text(turn: &DossierTurnRow) -> String {
    if opened_by_the_operator(turn) {
        return OPERATOR_OPENED.to_owned();
    }
    let what = what_else_opened(turn);
    format!(
        "{what}, not a message from the operator. The operator's latest message may already have been answered by an earlier turn, so weigh this turn against the whole thread and not against that message alone."
    )
}

pub fn turn_opening_section(inputs: &DossierInputs, turn_id: Option<i64>) -> AllocSection {
    let text = turn_id
        .and_then(|id| inputs.turns.iter().find(|turn| turn.turn_id == id))
        .map(opening_text)
        .unwrap_or_default();
    AllocSection {
        id: "turn_opening",
        heading: TURN_OPENING_HEADING.to_owned(),
        body: SectionBody::Fixed { text },
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::dossier::render::allocate::allocate;
    use crate::dossier::view::full_tests::{base_time, fixture};
    use chrono::Duration;

    fn note(minutes: i64, reaction: Option<&str>, label: Option<&str>) -> SessionNote {
        SessionNote {
            steerer_id: 6,
            steerer_name: "verifier".to_owned(),
            payload_text: Some(format!("  check the export ({minutes})  ")),
            created_at: base_time() + Duration::minutes(minutes),
            outcome_label: label.map(str::to_owned),
            reaction: reaction.map(str::to_owned),
        }
    }

    fn rendered(section: AllocSection) -> String {
        allocate(&[section], 4_000, &[]).text
    }

    #[test]
    fn earlier_notes_say_who_sent_each_one_and_how_the_session_took_it() {
        let mut inputs = fixture();
        inputs.notes = vec![
            note(1, Some("declined"), Some("wrong")),
            note(2, Some("complied"), Some("right")),
            note(3, None, None),
        ];
        let text = rendered(earlier_notes_section(&inputs));
        assert!(text.starts_with(EARLIER_NOTES_HEADING));
        assert!(text.contains("from verifier: check the export (1)\n  (the session said it did not apply; the note was judged wrong)"));
        assert!(text.contains("(the session acted on it; the note was judged right)"));
        assert!(text.contains("(the session's answer is not graded yet)"));
        let first = text.find("(1)").unwrap_or(usize::MAX);
        let last = text.find("(3)").unwrap_or(0);
        assert!(first < last, "the oldest note comes first");
    }

    #[test]
    fn a_session_with_no_notes_says_so() {
        let text = rendered(earlier_notes_section(&fixture()));
        assert!(text.ends_with("(none)"));
    }

    #[test]
    fn a_turn_the_operator_opened_is_named_as_theirs() {
        let text = rendered(turn_opening_section(&fixture(), Some(3)));
        assert_eq!(text, format!("{TURN_OPENING_HEADING}\n{OPERATOR_OPENED}"));
    }

    #[test]
    fn a_turn_a_background_task_opened_is_weighed_against_the_whole_thread() {
        let mut inputs = fixture();
        inputs.turns[2].opened_by = Some("machine".to_owned());
        inputs.turns[2].prompt =
            "<task-notification>\n<status>completed</status>\n</task-notification>".to_owned();
        let text = rendered(turn_opening_section(&inputs, Some(3)));
        assert!(text.contains("the result of a background task, not a message from the operator"));
        assert!(text.contains("may already have been answered by an earlier turn"));
    }

    #[test]
    fn an_opening_that_is_not_recognised_is_still_said_not_to_be_the_operator() {
        let mut inputs = fixture();
        inputs.turns[2].opened_by = Some("machine".to_owned());
        let text = rendered(turn_opening_section(&inputs, Some(3)));
        assert!(text.contains("something other than the operator, not a message"));
    }

    #[test]
    fn earlier_turns_start_at_the_last_one_the_operator_opened() {
        let text = rendered(earlier_turns_section(&fixture(), Some(3)));
        assert!(text.starts_with(EARLIER_TURNS_HEADING));
        assert!(text.contains("opened by the operator: \"and add the docs\""));
        assert!(text.contains("It ended: Docs written. I'll report back when CI finishes."));
        assert!(
            !text.contains("Refactor the store"),
            "an exchange before the operator's last one is left out"
        );
        assert!(
            !text.contains("Retry added."),
            "the turn being judged is not an earlier turn"
        );
    }

    #[test]
    fn turns_something_else_opened_since_then_follow_the_operators() {
        let mut inputs = fixture();
        inputs.turns[1].opened_by = Some("machine".to_owned());
        inputs.turns[1].prompt =
            "<task-notification>\n<status>completed</status>\n</task-notification>".to_owned();
        let text = rendered(earlier_turns_section(&inputs, Some(3)));
        let asked = text
            .find("opened by the operator: \"Refactor the store.")
            .unwrap_or(usize::MAX);
        let task = text
            .find("opened by the result of a background task")
            .unwrap_or(0);
        assert!(asked < task, "{text}");
        assert!(text.contains("It ended: Done. The store is refactored."));
    }

    #[test]
    fn many_turns_since_the_operators_keep_it_and_the_newest_three() {
        let mut inputs = fixture();
        let task = "<task-notification>\n<status>completed</status>\n</task-notification>";
        for n in 0..5 {
            let mut turn = inputs.turns[2].clone();
            turn.turn_id = 10 + n;
            turn.start_request_id = 20 + n;
            turn.opened_by = Some("machine".to_owned());
            turn.prompt = task.to_owned();
            turn.final_text = format!("background step {n} finished");
            inputs.turns.push(turn);
        }
        let mut judged = inputs.turns[2].clone();
        judged.turn_id = 99;
        judged.start_request_id = 90;
        inputs.turns.push(judged);
        let text = rendered(earlier_turns_section(&inputs, Some(99)));
        assert!(text.contains("opened by the operator: \"thanks, next add a retry\""));
        assert!(!text.contains("background step 1 finished"));
        for n in 2..5 {
            assert!(
                text.contains(&format!("background step {n} finished")),
                "{text}"
            );
        }
    }

    #[test]
    fn a_long_ending_keeps_its_last_part() {
        let mut inputs = fixture();
        inputs.turns[1].final_text =
            format!("{}The setting is now off.", "working on it. ".repeat(200));
        let text = rendered(earlier_turns_section(&inputs, Some(3)));
        assert!(text.contains("earlier characters not shown"));
        assert!(text.ends_with("The setting is now off."));
    }

    #[test]
    fn a_session_with_no_earlier_turn_says_so() {
        assert!(rendered(earlier_turns_section(&fixture(), Some(1))).ends_with("(none)"));
    }

    #[test]
    fn a_turn_that_cannot_be_found_adds_no_claim() {
        let text = rendered(turn_opening_section(&fixture(), Some(99)));
        assert!(text.ends_with("(none)"));
        assert!(rendered(turn_opening_section(&fixture(), None)).ends_with("(none)"));
    }
}
