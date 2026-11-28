use super::nothing_past;

#[test]
fn nothing_past_empty_checkout_is_idle() {
    assert!(nothing_past(5, None));
}

#[test]
fn nothing_past_newest_at_or_before_place_is_idle() {
    assert!(nothing_past(5, Some(5)));
    assert!(nothing_past(5, Some(3)));
}

#[test]
fn nothing_past_newer_event_wakes_the_tail() {
    assert!(!nothing_past(5, Some(6)));
}
