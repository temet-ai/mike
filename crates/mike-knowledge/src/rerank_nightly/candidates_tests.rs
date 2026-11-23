use serde_json::json;

use super::candidates::{KEPT_SOURCE_KINDS, candidate_query, contenders};
use crate::store::persistence::project_knowledge::SemanticCut;

const EVERY_SEARCH_KIND: [&str; 16] = [
    "artifact",
    "memory",
    "skill",
    "lesson",
    "note",
    "turn",
    "thread",
    "work_thread",
    "thread_entry",
    "question_answer",
    "unmet_ask",
    "correction",
    "learning",
    "session",
    "mail",
    "seat",
];

#[test]
fn candidate_query_asks_for_exactly_the_kinds_the_nightly_keeps() {
    let query = candidate_query("cache tuning".to_owned(), SemanticCut::default());

    assert_eq!(
        query.kinds,
        Some(KEPT_SOURCE_KINDS.iter().map(|k| (*k).to_owned()).collect())
    );
    assert_eq!(query.kind, None);
    assert_eq!(query.query, "cache tuning");
}

#[test]
fn contenders_keep_a_row_exactly_when_the_query_asks_for_its_kind() {
    let query = candidate_query(String::new(), SemanticCut::default());
    let asked = query.kinds.unwrap_or_default();
    let rows: Vec<_> = EVERY_SEARCH_KIND
        .iter()
        .map(|kind| json!({"source_kind": kind, "reference": format!("{kind}:1")}))
        .collect();

    let kept: Vec<String> = contenders(rows).into_iter().map(|c| c.kind).collect();

    let mut expected: Vec<String> = asked
        .iter()
        .map(|kind| {
            if kind == "thread_entry" {
                "entry".to_owned()
            } else {
                kind.clone()
            }
        })
        .collect();
    let mut kept = kept;
    expected.sort();
    kept.sort();
    assert_eq!(kept, expected);
}
