use super::*;

#[test]
fn operator_text_with_only_a_task_notification_is_none() {
    let prompt = "<system-reminder>\n[SYSTEM NOTIFICATION]\n<task-notification><status>completed</status></task-notification>\n</system-reminder>";
    assert_eq!(operator_text(prompt), None);
}

#[test]
fn operator_text_keeps_the_operator_words_around_a_reminder() {
    let prompt = "continue <system-reminder>lesson text</system-reminder>with the epic";
    assert_eq!(
        operator_text(prompt).as_deref(),
        Some("continue with the epic")
    );
}

#[test]
fn operator_text_with_an_unclosed_tag_drops_the_rest() {
    assert_eq!(
        operator_text("status<task-notification>done").as_deref(),
        Some("status")
    );
}

#[test]
fn typed_text_keeps_what_the_operator_typed() {
    assert_eq!(
        typed_text("continue <system-reminder>mail</system-reminder>the epic").as_deref(),
        Some("continue the epic")
    );
}

#[test]
fn typed_text_drops_mail_wake_registry_and_recap_text() {
    for prompt in [
        "Mail from session 3f2a: the build is done",
        "[mike-mail:abc] Mail from session 3f2a.\nThis is a peer's message",
        "<channel source=\"wake\" reason=\"x\">go</channel>",
        "[mike work registry] The main thread has been idle since 2026-11-14 while 1 agent(s) are open.",
        "The main thread has been idle since 2026-11-14 while 1 agent(s) are open.",
        "The user stepped away and is coming back. Recap in under 40 words what you did.",
        "<system-reminder>lesson</system-reminder>\nMail from session 3f2a: done",
    ] {
        assert_eq!(typed_text(prompt), None, "{prompt}");
    }
}

#[test]
fn typed_words_drops_machine_paragraphs_and_keeps_the_operator_ones() {
    let context = "fix the parser\n\n[mike-mail:1] Mail from session 3f2a.\nThis is a peer's message.\n\n<task-notification>x</task-notification>now run the tests";
    assert_eq!(typed_words(context), "fix the parser\n\nnow run the tests");
}

#[test]
fn tokenize_mixed_text_drops_stopwords_short_words_and_case() {
    let words: Vec<String> = tokenize("The Quick fox AND is on a Plan")
        .into_iter()
        .collect();
    assert_eq!(
        words,
        vec!["fox".to_owned(), "plan".to_owned(), "quick".to_owned()]
    );
}

#[test]
fn is_deictic_continuation_phrasing_matches_on_word_boundaries() {
    for prompt in [
        "Can we continue?",
        "carry on please",
        "Where are we on this",
        "what\u{2019}s the status",
        "pick up the open epic",
        "I thought about that decision",
    ] {
        assert!(is_deictic(prompt), "{prompt}");
    }
    for prompt in ["discontinue the service", "pick upload", "rename the field"] {
        assert!(!is_deictic(prompt), "{prompt}");
    }
}

#[test]
fn tickets_uppercase_prefix_and_two_digits_only_are_keys() {
    assert_eq!(
        tickets("see PROJ-1529 and AB-12, not ab-12, utf-8, gpt-4, X-99 or AB-1."),
        vec!["proj-1529", "ab-12"]
    );
}

#[test]
fn contains_bounded_needle_inside_a_longer_word_does_not_match() {
    assert!(contains_bounded("fix alpha-rollout now", "alpha-rollout"));
    assert!(!contains_bounded("fix alpha-rollouts now", "alpha-rollout"));
    assert!(!contains_bounded("anything", ""));
}
