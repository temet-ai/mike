use super::identifiers::query_identifiers;

fn found(query: &str) -> Vec<String> {
    query_identifiers(query)
}

#[test]
fn query_identifiers_ticket_id_is_found_beside_plain_words() {
    assert_eq!(found("ABC-1234 GitLab schedules"), vec!["ABC-1234"]);
}

#[test]
fn query_identifiers_hash_and_bang_numbers_keep_their_marker() {
    assert_eq!(found("see #123 and !45 today"), vec!["#123", "!45"]);
}

#[test]
fn query_identifiers_commit_hash_needs_a_digit_and_a_letter() {
    assert_eq!(found("fixed in d790ae58f"), vec!["d790ae58f"]);
    assert!(found("deadbeef cafebabe").is_empty());
    assert!(found("abc123").is_empty());
}

#[test]
fn query_identifiers_full_uuid_is_found() {
    let uuid = "36106608-f114-4e6e-91ef-182745aa7165";
    assert_eq!(found(&format!("session {uuid} notes")), vec![uuid]);
}

#[test]
fn query_identifiers_long_digit_run_is_found_and_short_one_is_not() {
    assert_eq!(
        found("20261109185000 search_grades migration"),
        vec!["20261109185000", "search_grades"]
    );
    assert!(found("12345 files").is_empty());
}

#[test]
fn query_identifiers_code_names_with_each_separator_are_found() {
    assert_eq!(
        found("search_grades rerank.cold_timeout_ms mark_used.sql src/api std::fmt"),
        vec![
            "search_grades",
            "rerank.cold_timeout_ms",
            "mark_used.sql",
            "src/api",
            "std::fmt"
        ]
    );
}

#[test]
fn query_identifiers_ordinary_words_give_nothing() {
    assert!(found("reranker").is_empty());
    assert!(found("design tokens").is_empty());
    assert!(found("deploy lock").is_empty());
    assert!(found("").is_empty());
    assert!(found("e.g. the end.").is_empty());
}

#[test]
fn query_identifiers_surrounding_quotes_and_punctuation_are_stripped() {
    assert_eq!(
        found("\"ABC-1234\", (search_grades). `mark_used.sql`? (#77)"),
        vec!["ABC-1234", "search_grades", "mark_used.sql", "#77"]
    );
}

#[test]
fn query_identifiers_repeats_differing_only_in_case_are_kept_once() {
    assert_eq!(found("ABC-1234 abc-1234 Abc-1234"), vec!["ABC-1234"]);
}

#[test]
fn query_identifiers_are_capped_at_five_in_query_order() {
    let query = "AA-1 BB-2 CC-3 DD-4 EE-5 FF-6 GG-7";
    assert_eq!(found(query), vec!["AA-1", "BB-2", "CC-3", "DD-4", "EE-5"]);
}
