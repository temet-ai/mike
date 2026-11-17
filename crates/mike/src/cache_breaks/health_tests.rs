use super::*;

fn group(kind: &str, cause: &str, stage: Option<&str>, breaks: i64, tokens: i64) -> CauseGroup {
    CauseGroup {
        kind: kind.to_owned(),
        cause: cause.to_owned(),
        stage: stage.map(str::to_owned),
        location: None,
        woken_by: None,
        breaks,
        sessions: 1,
        tokens_rewritten: tokens,
    }
}

#[test]
fn health_row_mike_breaks_are_degraded_and_name_the_top_stage() {
    let groups = [
        group("break", "mike", Some("self_identity"), 5, 3_000_000),
        group("break", "mike", Some("filter"), 2, 500_000),
        group("break", "client", None, 4, 900_000),
        group("expiry", "expired", None, 3, 2_000_000),
    ];
    let r = health_row(&groups);
    assert_eq!(r.state, CapabilityState::Degraded);
    assert!(
        r.detail
            .contains("7 time(s) and rewrote 3.5M tokens: self_identity 5 (3.0M), filter 2 (500k)"),
        "{}",
        r.detail
    );
    assert!(r.fix.contains(PAGE));
}

#[test]
fn health_row_without_mike_breaks_is_active() {
    let r = health_row(&[group("break", "client", None, 4, 900_000)]);
    assert_eq!(r.state, CapabilityState::Active);
    assert!(r.fix.is_empty());
}
