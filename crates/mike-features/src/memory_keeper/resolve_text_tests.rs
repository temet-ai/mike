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
