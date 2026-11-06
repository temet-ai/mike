use super::*;

#[test]
fn consecutive_timeouts_lengthen_the_pause_up_to_the_cap() {
    let b = TimeoutBackoff::default();
    assert_eq!(b.pause(Provider::Claude), Duration::ZERO);
    b.record(Provider::Claude, true);
    assert_eq!(b.pause(Provider::Claude), Duration::from_secs(2));
    b.record(Provider::Claude, true);
    assert_eq!(b.pause(Provider::Claude), Duration::from_secs(4));
    for _ in 0..20 {
        b.record(Provider::Claude, true);
    }
    assert_eq!(b.pause(Provider::Claude), MAX_DELAY);
}

#[test]
fn another_transport_and_any_non_timeout_outcome_are_unaffected() {
    let b = TimeoutBackoff::default();
    b.record(Provider::Codex, true);
    b.record(Provider::Codex, true);
    assert_eq!(b.pause(Provider::OpenRouter), Duration::ZERO);
    assert_eq!(b.record(Provider::Codex, false), 0);
    assert_eq!(b.pause(Provider::Codex), Duration::ZERO);
}
