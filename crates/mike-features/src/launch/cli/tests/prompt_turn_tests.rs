use super::super::prompt_turn::{TurnWatch, prefix};

#[test]
fn prefix_is_the_first_line_with_text_trimmed() {
    assert_eq!(
        prefix("\n  Your task brief (958 characters) is saved at /x.md\nRead it.").as_deref(),
        Some("Your task brief (958 characters) is saved at /x.md")
    );
}

#[test]
fn prefix_stops_at_sixty_characters_so_a_wrapped_paste_still_matches() {
    let long = "a".repeat(200);
    assert_eq!(prefix(&long).map(|p| p.chars().count()), Some(60));
}

#[test]
fn a_prompt_with_no_text_or_no_session_gives_nothing_to_watch() {
    assert!(prefix(" \n\t\n").is_none());
    assert!(TurnWatch::new(None, Some("do it")).is_none());
    assert!(TurnWatch::new(Some(uuid::Uuid::nil()), None).is_none());
    assert!(TurnWatch::new(Some(uuid::Uuid::nil()), Some("do it")).is_some());
}
