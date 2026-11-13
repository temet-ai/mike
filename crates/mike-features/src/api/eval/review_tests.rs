use super::{NextMessageState, TextState, assistant_state, next_message_state, prompt_state};

#[test]
fn assistant_text_that_was_never_written_is_an_empty_turn() {
    assert_eq!(assistant_state(None, Some(0)), TextState::EmptyTurn);
    assert_eq!(assistant_state(Some(""), Some(0)), TextState::EmptyTurn);
}

#[test]
fn assistant_text_that_the_facts_say_existed_but_is_gone_is_missing() {
    assert_eq!(assistant_state(None, Some(120)), TextState::Missing);
    assert_eq!(assistant_state(Some("  "), None), TextState::Missing);
}

#[test]
fn assistant_text_present_wins_over_the_fact_count() {
    assert_eq!(assistant_state(Some("done"), Some(0)), TextState::Present);
}

#[test]
fn a_blank_prompt_is_missing_not_empty() {
    assert_eq!(prompt_state(None), TextState::Missing);
    assert_eq!(prompt_state(Some(" ")), TextState::Missing);
    assert_eq!(prompt_state(Some("fix it")), TextState::Present);
}

#[test]
fn next_message_states_follow_the_turn_facts() {
    assert_eq!(
        next_message_state(true, Some("resolved"), Some("thanks")),
        NextMessageState::Present
    );
    assert_eq!(
        next_message_state(true, Some("resolved"), None),
        NextMessageState::Missing
    );
    assert_eq!(
        next_message_state(true, Some("none_session_settled"), None),
        NextMessageState::None
    );
    assert_eq!(
        next_message_state(true, Some("pending"), None),
        NextMessageState::Pending
    );
    assert_eq!(
        next_message_state(false, None, None),
        NextMessageState::Unknown
    );
}
