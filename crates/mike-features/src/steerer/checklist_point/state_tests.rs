use super::questions::key;
use super::state::{
    Criterion, Evidence, intake_criteria, is_rule, state, tail_lines, thread_criteria,
};
use crate::store::persistence::acceptance_criteria::{
    AcceptanceCriteriaMap, AcceptanceCriterionRow,
};

fn criterion(text: &str, inferred: bool) -> Criterion {
    Criterion {
        text: text.to_owned(),
        inferred,
        context: None,
    }
}

fn with_words(text: &str, words: &str) -> Criterion {
    Criterion {
        context: Some(words.to_owned()),
        ..criterion(text, false)
    }
}

fn intake_map(rows: &[(&str, Option<&str>)]) -> AcceptanceCriteriaMap {
    AcceptanceCriteriaMap {
        id: 1,
        version: 1,
        ask_entry_count: 1,
        model: None,
        output_tokens: None,
        created_at: chrono::Utc::now(),
        criteria: rows
            .iter()
            .zip(0..)
            .map(|((text, span), ord)| AcceptanceCriterionRow {
                ord,
                criterion: (*text).to_owned(),
                provenance: if span.is_some() {
                    "operator"
                } else {
                    "inferred"
                }
                .to_owned(),
                operator_span: span.map(str::to_owned),
            })
            .collect(),
    }
}

#[test]
fn intake_criteria_gives_one_criterion_per_stored_row_with_the_operators_words() {
    let map = intake_map(&[
        ("The panel is reachable.", Some("no direct URL")),
        ("It uses shared primitives.", None),
        ("  ", None),
        ("It loads under a second.", Some("make it fast")),
    ]);
    assert_eq!(
        intake_criteria(&map),
        vec![
            with_words("The panel is reachable.", "no direct URL"),
            criterion("It uses shared primitives.", true),
            with_words("It loads under a second.", "make it fast"),
        ]
    );
}

#[test]
fn thread_criteria_keeps_list_items_or_every_plain_line() {
    let headed = thread_criteria("Acceptance criteria:\n- It ships.\n- It is proven.");
    assert_eq!(
        headed,
        vec![
            criterion("It ships.", false),
            criterion("It is proven.", false)
        ]
    );
    let plain = thread_criteria("It ships.\nIt is proven.");
    assert_eq!(
        plain,
        vec![
            criterion("It ships.", false),
            criterion("It is proven.", false)
        ]
    );
}

#[test]
fn thread_criteria_reads_list_markers_and_the_inferred_tag() {
    let found = thread_criteria(
        "- The page loads.\n2. It renders.\n- [inferred, not stated by the operator] It is fast.\n\n",
    );
    assert_eq!(
        found,
        vec![
            criterion("The page loads.", false),
            criterion("It renders.", false),
            criterion("It is fast.", true),
        ]
    );
}

#[test]
fn thread_criteria_drops_the_rendered_intake_heading_and_folds_their_words() {
    let block = "Acceptance criteria for this session, written from the operator's own words by a worker that never saw this session's plan or restatement.\n\n- [operator asked] The comms panel is reachable from the session page.\n  (their words: \"add a comms panel\")\n- [inferred, not stated by the operator] It uses shared primitives.";
    assert_eq!(
        thread_criteria(block),
        vec![
            with_words(
                "The comms panel is reachable from the session page.",
                "add a comms panel"
            ),
            criterion("It uses shared primitives.", true),
        ]
    );
}

#[test]
fn thread_criteria_plain_lines_drop_headings_and_their_words_lines() {
    let block = "Acceptance criteria for this session, written from the operator's own words.\nPR #256 merged and deployed.\n(their words: \"ship it\")\nWhat must hold:\nNothing on omit is touched.";
    assert_eq!(
        thread_criteria(block),
        vec![
            with_words("PR #256 merged and deployed.", "ship it"),
            criterion("Nothing on omit is touched.", false),
        ]
    );
}

#[test]
fn thread_criteria_joins_wrapped_list_items_and_skips_prose_between_lists() {
    let block = "The six numbered requirements of temet-ai/mike#110, \"What a revisit needs to establish\":\n\n1. A supersedes/refines relationship. Must model \"B updates A\"\n   and must not be a cosine threshold alone.\n2. Provenance at write time.\n\nOperator's acceptance criteria for this run:\n- AC1 — A superseding lesson replaces the old one.";
    assert_eq!(
        thread_criteria(block),
        vec![
            criterion(
                "A supersedes/refines relationship. Must model \"B updates A\" and must not be a cosine threshold alone.",
                false
            ),
            criterion("Provenance at write time.", false),
            criterion("AC1 — A superseding lesson replaces the old one.", false),
        ]
    );
}

#[test]
fn is_rule_reads_the_opening_word() {
    for rule in [
        "Nothing on omit is touched.",
        "No table keys a project by a path string.",
        "Never push with --no-verify.",
        "Do not restart the proxy.",
        "Don't drop rows.",
        "don\u{2019}t drop rows.",
        "without a migration",
    ] {
        assert!(is_rule(rule), "{rule}");
    }
    for outcome in [
        "PR #256 merged.",
        "Notes are shown on the page.",
        "Nowhere is it slow.",
        "Doing it twice is fine.",
    ] {
        assert!(!is_rule(outcome), "{outcome}");
    }
}

#[test]
fn state_carries_the_end_of_the_digest_every_criterion_and_the_facts() {
    let long = format!("{}LAST CALL", "x".repeat(9_000));
    let earlier = vec!["Merged #255.".to_owned()];
    let value = state(
        &[
            with_words("It loads", "make it load"),
            criterion("No secrets leak", false),
        ],
        &Evidence {
            trajectory: &long,
            turn_end: "Done. Should I deploy it now?",
            earlier_turns: &earlier,
        },
    );
    let trajectory = value["evidence"]["trajectory"].as_str().unwrap_or_default();
    assert!(trajectory.ends_with("LAST CALL"));
    assert_eq!(value["criteria"][key(0)]["text"], "It loads");
    assert_eq!(value["criteria"][key(0)]["their_words"], "make it load");
    assert_eq!(value["criteria"][key(0)]["kind"], "outcome");
    assert_eq!(value["criteria"][key(1)]["kind"], "rule");
    assert_eq!(value["facts"]["ends_with_operator_question"], true);
    assert_eq!(value["facts"]["earlier_turns"][0], "Merged #255.");
}

#[test]
fn tail_lines_drops_the_oldest_lines_whole() {
    let digest = "1. first call\n2. second call\n3. third call";
    assert_eq!(tail_lines(digest, 28), "2. second call\n3. third call");
    assert_eq!(tail_lines(digest, 100), digest);
    assert_eq!(tail_lines("abcdef", 3), "def");
}
