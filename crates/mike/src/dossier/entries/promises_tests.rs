use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use super::promises::{has_registry_coverage, open_entry, promise_span};
use crate::store::persistence::dossier_entries::{BackingItemRow, PromiseSourceRow};

fn source(text: &str, stop_request_id: Option<i64>, at: DateTime<Utc>) -> PromiseSourceRow {
    PromiseSourceRow {
        turn_id: 42,
        session_id: 3,
        stop_request_id,
        stop_completed_at: Some(at),
        closed_at: at + Duration::seconds(5),
        final_text: text.to_owned(),
        text_spans: json!([]),
        session_class: "operator".to_owned(),
    }
}

fn item(id: i64, launched: DateTime<Utc>, finished: Option<DateTime<Utc>>) -> BackingItemRow {
    BackingItemRow {
        work_item_id: id,
        session_id: 3,
        kind: "agent".to_owned(),
        task_id: format!("task-{id}"),
        owner_agent_id: None,
        description: Some("run the suite".to_owned()),
        launched_at: launched,
        finished_at: finished,
    }
}

#[test]
fn span_marks_the_wait_phrase() {
    let text = "Tests are running. I'll report back when they finish.";
    let span = promise_span(&source(text, Some(1), Utc::now()));
    let (start, end) = span.unwrap_or((0, 0));
    assert!(end > start, "a wait phrase has a span");
    let phrase: String = text.chars().skip(start).take(end - start).collect();
    assert!(phrase.to_lowercase().contains("report"), "span {phrase:?}");
}

#[test]
fn coverage_is_false_before_the_registry_existed() {
    let first = Utc::now();
    assert!(!has_registry_coverage(
        first - Duration::hours(1),
        Some(first)
    ));
    assert!(has_registry_coverage(
        first + Duration::hours(1),
        Some(first)
    ));
    assert!(!has_registry_coverage(first, None));
}

#[test]
fn backing_lists_items_running_at_the_promise() {
    let at = Utc::now();
    let backing = vec![
        item(1, at - Duration::minutes(10), None),
        item(
            2,
            at - Duration::minutes(10),
            Some(at - Duration::minutes(1)),
        ),
        item(3, at + Duration::minutes(1), None),
    ];
    let entry = open_entry(
        &source("I'll check back once it finishes.", Some(9), at),
        &backing,
        Some(at - Duration::days(1)),
    );
    assert_eq!(entry.data["registry_coverage"], json!(true));
    let ids: Vec<i64> = entry.data["backing"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i["work_item_id"].as_i64())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(ids, vec![1]);
    assert_eq!(entry.source_ref, "requests.requests:9");
    assert_eq!(entry.dedup_key, "turn:42");
    assert_eq!(entry.effective_at, at);
}

#[test]
fn without_coverage_backing_is_null_not_empty() {
    let at = Utc::now();
    let entry = open_entry(&source("waiting for the build", None, at), &[], None);
    assert_eq!(entry.data["registry_coverage"], json!(false));
    assert!(entry.data["backing"].is_null());
    assert_eq!(entry.source_ref, "memory.operator_turns:42");
}
