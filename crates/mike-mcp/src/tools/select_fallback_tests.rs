use serde_json::json;

use super::*;

fn selectors(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn resolve_under_wrapper_pointer_missing_the_wrapper_key_is_retried_under_it() {
    let envelope = json!({"content_search": {"sessions": [{"id": "a"}]}});
    let resolved = resolve_under_wrapper(&envelope, &selectors(&["/sessions/*/id"]));
    assert_eq!(
        resolved.selectors,
        selectors(&["/content_search/sessions/*/id"])
    );
    assert_eq!(
        resolved.corrected,
        vec![(
            "/sessions/*/id".to_string(),
            "/content_search/sessions/*/id".to_string()
        )]
    );
}

#[test]
fn resolve_under_wrapper_pointer_matching_at_the_root_is_never_rewritten() {
    let envelope = json!({"find": {"find": {"x": 1}, "x": 2}});
    let resolved = resolve_under_wrapper(&envelope, &selectors(&["/find/x"]));
    assert_eq!(resolved.selectors, selectors(&["/find/x"]));
    assert!(resolved.corrected.is_empty());
}

#[test]
fn resolve_under_wrapper_two_data_keys_get_no_retry() {
    let envelope = json!({"find": {"matches": [1]}, "extra": {"matches": [2]}});
    let resolved = resolve_under_wrapper(&envelope, &selectors(&["/matches"]));
    assert_eq!(resolved.selectors, selectors(&["/matches"]));
    assert!(resolved.corrected.is_empty());
}

#[test]
fn resolve_under_wrapper_bookkeeping_keys_do_not_count_as_data() {
    let envelope = json!({
        "knowledge": {"items": [1]},
        "window_note": "n",
        "next_cursor": "c"
    });
    let resolved = resolve_under_wrapper(&envelope, &selectors(&["/items"]));
    assert_eq!(resolved.selectors, selectors(&["/knowledge/items"]));
}

#[test]
fn resolve_under_wrapper_pointer_matching_nowhere_is_left_for_the_unmatched_report() {
    let envelope = json!({"find": {"matches": []}});
    let resolved = resolve_under_wrapper(&envelope, &selectors(&["/nope"]));
    assert_eq!(resolved.selectors, selectors(&["/nope"]));
    assert!(resolved.corrected.is_empty());
}

#[test]
fn resolve_under_wrapper_escapes_a_wrapper_key_containing_a_slash() {
    let envelope = json!({"a/b": {"x": 1}});
    let resolved = resolve_under_wrapper(&envelope, &selectors(&["/x"]));
    assert_eq!(resolved.selectors, selectors(&["/a~1b/x"]));
}
