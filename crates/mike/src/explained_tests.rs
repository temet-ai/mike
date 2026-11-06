use super::*;

#[test]
fn first_time_answers_true_once_per_session() {
    assert!(first_time("explained-once", GIT_READ_HINT));
    assert!(!first_time("explained-once", GIT_READ_HINT));
    assert!(first_time("explained-once-other", GIT_READ_HINT));
}

#[test]
fn a_compaction_lets_every_explanation_be_given_again() {
    observe("explained-compact", 80);
    assert!(first_time("explained-compact", MEMORY_TAGS));
    observe("explained-compact", 90);
    assert!(!first_time("explained-compact", MEMORY_TAGS));
    observe("explained-compact", 3);
    assert!(first_time("explained-compact", MEMORY_TAGS));
}

#[test]
fn new_topics_names_only_topics_the_session_has_not_had() {
    let topics = |t: &[&str]| t.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        new_topics("explained-topics", &topics(&["a", "b"])),
        topics(&["a", "b"])
    );
    assert_eq!(
        new_topics("explained-topics", &topics(&["b", "c"])),
        topics(&["c"])
    );
}

#[test]
fn observe_body_counts_messages_and_ignores_a_body_without_them() {
    observe_body(
        "explained-body",
        br#"{"messages":[{"role":"user","content":"a"},{"role":"assistant","content":"b"}]}"#,
    );
    assert!(first_time("explained-body", GIT_READ_HINT));
    observe_body("explained-body", b"not json");
    assert!(!first_time("explained-body", GIT_READ_HINT));
    observe_body(
        "explained-body",
        br#"{"messages":[{"role":"user","content":"a"}]}"#,
    );
    assert!(first_time("explained-body", GIT_READ_HINT));
}
