use super::super::prompt::{BoxState, box_state};

const PROMPT: &str = "Read the brief and start on the first task\nthen report";

#[test]
fn box_state_prompt_still_in_box_is_unsent() {
    let screen = "banner\n----\n\u{276f} Read the brief and start on the first\n----\n";
    assert_eq!(box_state(screen, PROMPT), BoxState::Unsent);
}

#[test]
fn box_state_no_break_space_after_the_mark_is_unsent() {
    let screen = "\u{276f}\u{a0}Read the brief and start on the first\n";
    assert_eq!(box_state(screen, PROMPT), BoxState::Unsent);
}

#[test]
fn box_state_collapsed_paste_is_unsent() {
    let screen = "\u{276f} [Pasted text #1 +40 lines]\n";
    assert_eq!(box_state(screen, PROMPT), BoxState::Unsent);
}

#[test]
fn box_state_empty_box_under_the_echoed_prompt_is_taken() {
    let screen =
        "\u{276f} Read the brief and start on the first\n\u{25cf} ok\n----\n\u{276f} \n----\n";
    assert_eq!(box_state(screen, PROMPT), BoxState::Taken);
}

#[test]
fn box_state_before_the_box_is_drawn_is_not_drawn() {
    assert_eq!(box_state("starting", PROMPT), BoxState::NotDrawn);
}
