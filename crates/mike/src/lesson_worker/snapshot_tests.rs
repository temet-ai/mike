//! Tests for [`super::snapshot::LessonSnapshot`]'s agent-scoped lookups,
//! persona resolution, and first-injection claim.

use super::snapshot::LessonSnapshot;

fn fixture_snapshot() -> LessonSnapshot {
    LessonSnapshot::from_agent_fixtures(
        [("code-reviewer".to_string(), 7_i64)],
        [(
            7_i64,
            vec![
                (
                    "correction".to_string(),
                    "never approve without running tests".to_string(),
                ),
                ("convention".to_string(), "prefer set-based SQL".to_string()),
            ],
        )],
    )
}

#[test]
fn resolve_persona_resolves_an_agent_by_its_own_name() {
    let snap = LessonSnapshot::from_agent_fixtures(
        [("rust-developer".to_string(), 3_i64)],
        Vec::<(i64, Vec<(String, String)>)>::new(),
    );
    assert_eq!(snap.resolve_persona("rust-developer"), Some(3));
    assert_eq!(snap.resolve_persona("fleet-worker"), None);
}

#[test]
fn agent_lesson_served_by_client_is_per_session_and_persona() {
    let snap = fixture_snapshot();
    assert!(!snap.agent_lesson_served_by_client("session-C", "code-reviewer"));
    assert!(snap.mark_agent_lesson_served_by_client("session-C", "Code-Reviewer"));
    assert!(snap.agent_lesson_served_by_client("session-C", "code-reviewer"));
    assert!(!snap.agent_lesson_served_by_client("session-D", "code-reviewer"));
    assert!(!snap.agent_lesson_served_by_client("session-C", "other-persona"));
}

#[test]
fn agent_lessons_for_scopes_to_the_resolved_agent() {
    let snap = fixture_snapshot();
    let lessons = snap.agent_lessons_for(7);
    assert_eq!(lessons.len(), 2);
    assert!(lessons.iter().any(|l| l.text.contains("running tests")));
}

#[test]
fn agent_lessons_for_unknown_agent_is_empty() {
    let snap = fixture_snapshot();
    assert!(snap.agent_lessons_for(999).is_empty());
}

#[test]
fn resolve_persona_is_case_insensitive() {
    let snap = fixture_snapshot();
    assert_eq!(snap.resolve_persona("code-reviewer"), Some(7));
    assert_eq!(snap.resolve_persona("Code-Reviewer"), Some(7));
    assert_eq!(snap.resolve_persona("CODE-REVIEWER"), Some(7));
}

#[test]
fn resolve_persona_unaliased_returns_none() {
    let snap = fixture_snapshot();
    assert_eq!(
        snap.resolve_persona("some free-text persona sentence"),
        None
    );
}

#[test]
fn mark_agent_lesson_injected_fires_once_per_session_and_persona() {
    let snap = fixture_snapshot();
    assert!(
        snap.mark_agent_lesson_injected("session-A", "code-reviewer"),
        "first claim for (session-A, code-reviewer) must succeed"
    );
    assert!(
        !snap.mark_agent_lesson_injected("session-A", "code-reviewer"),
        "second claim for the same pair must be refused"
    );
    // Case-insensitive: a different-cased persona hits the same claim.
    assert!(
        !snap.mark_agent_lesson_injected("session-A", "Code-Reviewer"),
        "differently-cased persona must resolve to the same claim key"
    );
    // A different session or a different persona is a distinct claim.
    assert!(snap.mark_agent_lesson_injected("session-B", "code-reviewer"));
    assert!(snap.mark_agent_lesson_injected("session-A", "other-persona"));
}

#[test]
fn explicit_session_project_overrides_machine_local_path() {
    let snapshot = LessonSnapshot::default()
        .with_session_project("mac-session".into(), "mike:logical-project".into());
    assert_eq!(
        snapshot.project_for_session("mac-session", "/Users/dev/app"),
        "mike:logical-project"
    );
    assert_eq!(
        snapshot.project_for_session("legacy-session", "/work/app"),
        "/work/app"
    );
}
