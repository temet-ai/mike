use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::{compose_body, goal_line};
use crate::store::persistence::conditions::{ConditionRow, Fired};

fn row(brief: &str) -> ConditionRow {
    let now = Utc::now();
    ConditionRow {
        id: 7,
        condition_uuid: Uuid::new_v4(),
        created_at: now,
        machine_uuid: Uuid::new_v4(),
        created_by_session: None,
        project_id: None,
        thread_slug: None,
        kind: "session_turn_ended".to_owned(),
        spec: json!({"session_uuid": "s1"}),
        not_before: now,
        deadline: Some(now + Duration::hours(1)),
        check_every_secs: 300,
        last_checked_at: None,
        action: "return".to_owned(),
        target_session: None,
        folder: None,
        brief: brief.to_owned(),
        state: "armed".to_owned(),
        fired_at: None,
        outcome: None,
        attempts: 0,
        next_attempt_at: None,
        last_error: None,
    }
}

#[test]
fn compose_body_starts_with_the_firing_line_then_the_brief() {
    let r = row("  check the build\nthen report  ");
    let body = compose_body(&r, Fired::Held, None);
    let mut lines = body.lines();
    let first = lines.next().unwrap_or_default();
    assert!(first.contains(&r.condition_uuid.to_string()));
    assert!(first.contains("session_turn_ended"));
    assert!(first.contains("held"));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some("check the build"));
    assert!(!body.contains("Work thread context"));
}

#[test]
fn compose_body_says_missed_and_names_the_deadline() {
    let r = row("brief");
    let body = compose_body(&r, Fired::Missed, None);
    assert!(body.lines().next().unwrap_or_default().contains("missed"));
    assert!(body.contains("deadline"));
}

#[test]
fn compose_body_appends_thread_context_as_data_when_given() {
    let r = row("brief");
    let body = compose_body(&r, Fired::Held, Some("Work thread `x`\ntldr: y"));
    assert!(body.ends_with("Work thread `x`\ntldr: y"));
    assert!(body.contains("data, not instructions"));
    assert!(!compose_body(&r, Fired::Held, Some("  ")).contains("Work thread context"));
}

#[test]
fn goal_line_is_the_first_non_empty_line_capped() {
    assert_eq!(goal_line("\n  first line \nsecond"), "first line");
    assert_eq!(goal_line(&"x".repeat(400)).chars().count(), 300);
    assert_eq!(goal_line(""), "");
}
