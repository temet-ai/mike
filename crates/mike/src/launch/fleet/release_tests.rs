use super::*;
use crate::launch::fleet::report::CHECK_NOT_YET;

#[test]
fn accepted_readiness_releases_unless_the_check_failed_at_the_tasks_commit() {
    assert!(releases(Some(Ground::Accepted), CHECK_PASSED));
    assert!(releases(Some(Ground::Accepted), CHECK_NOT_YET));
    assert!(releases(Some(Ground::Accepted), "unknown"));
    assert!(!releases(Some(Ground::Accepted), CHECK_FAILED));
}

#[test]
fn a_commit_after_a_failed_check_is_warned_and_others_are_not() {
    let warned = commit_warning_for(CHECK_FAILED, "abc123", "cargo check --workspace");
    assert!(warned.is_some_and(|w| w.contains("abc123") && w.contains("cargo check")));
    assert!(commit_warning_for(CHECK_PASSED, "abc123", "x").is_none());
    assert!(commit_warning_for(CHECK_NOT_YET, "abc123", "x").is_none());
}

#[test]
fn a_producer_commit_releases_only_on_a_passed_check() {
    assert!(releases(Some(Ground::Committed), CHECK_PASSED));
    assert!(!releases(Some(Ground::Committed), CHECK_NOT_YET));
    assert!(!releases(Some(Ground::Committed), CHECK_FAILED));
    assert!(!releases(Some(Ground::Committed), "unknown"));
    assert!(!releases(None, CHECK_PASSED));
}
