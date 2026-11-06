use chrono::{Duration, Utc};
use serde_json::json;

use super::observation::{CallFact, observe};

fn call(id: i64, tool: &str, input: serde_json::Value, has_result: bool) -> CallFact {
    CallFact {
        use_id: id,
        at: Utc::now() + Duration::seconds(id),
        tool_name: tool.to_owned(),
        input,
        has_result,
    }
}

#[test]
fn a_turn_without_a_change_has_no_observation_fact() {
    let o = observe(&[call(1, "Read", json!({"file_path": "a.rs"}), true)]);
    assert_eq!(o.observed_after_change, None);
    assert_eq!(o.last_change_use_id, None);
}

#[test]
fn a_test_after_the_last_edit_is_observed() {
    let o = observe(&[
        call(1, "Edit", json!({}), true),
        call(2, "Bash", json!({"command": "cargo nextest run"}), true),
    ]);
    assert_eq!(o.observed_after_change, Some(true));
    assert_eq!(o.last_change_use_id, Some(1));
    assert_eq!(o.last_observation_use_id, Some(2));
    assert_eq!(o.observation_kind, Some("test"));
}

#[test]
fn a_test_before_the_last_edit_does_not_count() {
    let o = observe(&[
        call(1, "Bash", json!({"command": "cargo nextest run"}), true),
        call(2, "Edit", json!({}), true),
    ]);
    assert_eq!(o.observed_after_change, Some(false));
    assert_eq!(o.last_observation_use_id, None);
}

#[test]
fn an_observation_without_a_result_observed_nothing() {
    let o = observe(&[
        call(1, "Edit", json!({}), true),
        call(2, "Bash", json!({"command": "cargo nextest run"}), false),
    ]);
    assert_eq!(o.observed_after_change, Some(false));
}

#[test]
fn reading_back_the_written_file_observes_it() {
    let o = observe(&[
        call(1, "Write", json!({"file_path": "/m/note.md"}), true),
        call(2, "Bash", json!({"command": "cat /m/note.md"}), true),
    ]);
    assert_eq!(o.observed_after_change, Some(true));
    assert_eq!(o.observation_kind, Some("read_back"));
    assert_eq!(o.last_observation_use_id, Some(2));
}

#[test]
fn reading_back_any_file_of_the_change_batch_observes_it() {
    let o = observe(&[
        call(1, "Write", json!({"file_path": "/m/note.md"}), true),
        call(
            2,
            "Bash",
            json!({"command": "echo x >> /m/MEMORY.md"}),
            true,
        ),
        call(
            3,
            "Bash",
            json!({"command": "head -5 /m/note.md; grep -c x /m/MEMORY.md"}),
            true,
        ),
    ]);
    assert_eq!(o.observed_after_change, Some(true));
}

#[test]
fn reading_an_unrelated_file_does_not_observe_the_change() {
    let o = observe(&[
        call(1, "Write", json!({"file_path": "/m/note.md"}), true),
        call(2, "Bash", json!({"command": "cat /m/other.md"}), true),
        call(3, "Read", json!({"file_path": "/m/other.md"}), true),
    ]);
    assert_eq!(o.observed_after_change, Some(false));
}

#[test]
fn a_read_back_without_a_result_or_before_the_change_does_not_count() {
    let before = observe(&[
        call(1, "Bash", json!({"command": "cat /m/note.md"}), true),
        call(2, "Write", json!({"file_path": "/m/note.md"}), true),
    ]);
    assert_eq!(before.observed_after_change, Some(false));
    let unanswered = observe(&[
        call(1, "Write", json!({"file_path": "/m/note.md"}), true),
        call(2, "Read", json!({"file_path": "/m/note.md"}), false),
    ]);
    assert_eq!(unanswered.observed_after_change, Some(false));
}

#[test]
fn a_read_tool_call_of_the_written_path_observes_it() {
    let o = observe(&[
        call(1, "Edit", json!({"file_path": "docs/a.md"}), true),
        call(2, "Read", json!({"file_path": "/repo/docs/a.md"}), true),
    ]);
    assert_eq!(o.observed_after_change, Some(true));
}
