use serde_json::{Value, json};

use super::shown::{SHOWN_CHARS, SHOWN_RESULTS, set_from_items};

fn returned(count: usize) -> Value {
    Value::Array(
        (0..count)
            .map(|n| json!({"reference": format!("lesson:{n}"), "score": 0.5}))
            .collect(),
    )
}

fn items(count: usize, excerpt: &str) -> Value {
    Value::Array(
        (0..count)
            .map(|n| json!({"reference": format!("lesson:{n}"), "excerpt": excerpt}))
            .collect(),
    )
}

#[test]
fn set_from_items_stores_each_excerpt_next_to_its_reference() {
    let mut log = returned(2);

    set_from_items(&mut log, &items(2, "what was shown"));

    assert_eq!(log[0]["shown"], "what was shown");
    assert_eq!(log[1]["shown"], "what was shown");
    assert_eq!(log[0]["score"], 0.5);
}

#[test]
fn set_from_items_cuts_a_long_excerpt_to_1500_characters() {
    let mut log = returned(1);

    set_from_items(&mut log, &items(1, &"é".repeat(SHOWN_CHARS + 200)));

    assert_eq!(
        log[0]["shown"].as_str().map(|text| text.chars().count()),
        Some(SHOWN_CHARS)
    );
}

#[test]
fn set_from_items_stores_shown_for_the_first_ten_results_only() {
    let mut log = returned(SHOWN_RESULTS + 2);

    set_from_items(&mut log, &items(SHOWN_RESULTS + 2, "text"));

    let with_shown = log.as_array().map(|entries| {
        entries
            .iter()
            .filter(|entry| entry.get("shown").is_some())
            .count()
    });
    assert_eq!(with_shown, Some(SHOWN_RESULTS));
    assert!(log[SHOWN_RESULTS].get("shown").is_none());
}

#[test]
fn set_from_items_replaces_what_an_earlier_shape_stored_and_skips_pointers() {
    let mut log = returned(2);
    set_from_items(&mut log, &items(2, "full excerpt"));
    let delivered = json!([
        {"reference": "lesson:0", "excerpt": "best passage"},
        {"reference": "lesson:1", "line": "lesson · 2026-11-10 · title"}
    ]);

    set_from_items(&mut log, &delivered);

    assert_eq!(log[0]["shown"], "best passage");
    assert!(log[1].get("shown").is_none());
}

#[test]
fn set_from_items_ignores_an_item_for_another_reference_and_blank_excerpts() {
    let mut log = returned(2);
    let delivered = json!([
        {"reference": "lesson:other", "excerpt": "wrong result"},
        {"reference": "lesson:1", "excerpt": "   "}
    ]);

    set_from_items(&mut log, &delivered);

    assert!(log[0].get("shown").is_none());
    assert!(log[1].get("shown").is_none());
}
