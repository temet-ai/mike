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

fn check_event(
    id: i64,
    command: &str,
    sha: &str,
    passed: bool,
) -> crate::change_record::ChangeEvent {
    crate::change_record::ChangeEvent {
        id,
        project_id: 1,
        checkout: "/srv/project".into(),
        at: chrono::Utc::now(),
        kind: ChangeKind::Check,
        session_id: None,
        run_id: Some("run".into()),
        task: None,
        paths: vec![],
        subject: String::new(),
        detail: serde_json::json!({"command": command, "commit": sha, "passed": passed}),
        before_sha: None,
        after_sha: Some(sha.into()),
    }
}

#[test]
fn a_gate_failing_at_a_producers_commit_does_not_hold_back_its_release_when_the_fleet_check_passed()
{
    let check = "for f in src/*.mjs bin/*.mjs; do node --check \"$f\" || exit 1; done";
    let sha = "80f7da3545a78d9aafabd6b8c1ef91b991d46f3d";
    let newest_first = vec![
        check_event(4899, "npm test", sha, false),
        check_event(4898, check, sha, true),
    ];
    assert_eq!(
        Gate(Some(newest_first.clone())).word(Some(sha)),
        CHECK_FAILED,
        "reading every check event, the gate's failure is taken for the check's"
    );
    let word = Gate(Some(of_the_check(newest_first, check))).word(Some(sha));
    assert_eq!(word, CHECK_PASSED);
    assert!(releases(Some(Ground::Accepted), &word));
}
