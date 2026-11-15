use chrono::Utc;
use serde_json::json;

use super::apply_kind_problems;
use crate::store::persistence::project_knowledge::KnowledgeKindRow;
use crate::web_base::capability::{CapabilityRow, CapabilityState};

fn kind(name: &str) -> KnowledgeKindRow {
    KnowledgeKindRow {
        kind: name.to_owned(),
        label: name.to_owned(),
        description: None,
        body_view: format!("memory.knowledge_body_{name}_v"),
        settle_secs: 0,
        enabled: true,
        built_in: false,
        generated_view: true,
        source_table: None,
        key_column: None,
        title_column: None,
        body_columns: None,
        project_column: None,
        created_at: Utc::now(),
        items: 4,
        embedded: 4,
        waiting: 0,
        failed: 0,
        newest_error: None,
        newest_error_at: None,
        oldest_waiting_ready_at: None,
        behind: false,
        watches: json!([]),
    }
}

fn active_row() -> CapabilityRow {
    CapabilityRow {
        id: "project_knowledge",
        label: "Project knowledge embeddings",
        state: CapabilityState::Active,
        detail: "9 sources embedded".to_owned(),
        fix: String::new(),
    }
}

#[test]
fn kind_problems_healthy_kinds_leave_the_row_unchanged() {
    let mut row = active_row();

    apply_kind_problems(&mut row, &[kind("widgets")]);

    assert_eq!(row.state, CapabilityState::Active);
    assert_eq!(row.detail, "9 sources embedded");
    assert!(row.fix.is_empty());
}

#[test]
fn kind_problems_failing_kind_degrades_the_row_with_counts_error_and_remedy() {
    let mut row = active_row();
    let mut failing = kind("widgets");
    failing.failed = 2;
    failing.embedded = 2;
    failing.newest_error = Some("embedding service refused".to_owned());

    apply_kind_problems(&mut row, &[failing]);

    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(
        row.detail
            .contains("widgets (4 items, 2 embedded, 0 waiting, 2 failed")
    );
    assert!(row.detail.contains("embedding service refused"));
    assert!(row.fix.contains("/settings/knowledge"));
}

#[test]
fn kind_problems_disabled_kind_is_not_reported() {
    let mut row = active_row();
    let mut off = kind("widgets");
    off.enabled = false;
    off.failed = 3;
    off.behind = true;

    apply_kind_problems(&mut row, &[off]);

    assert_eq!(row.state, CapabilityState::Active);
}

#[test]
fn kind_problems_behind_kind_names_the_oldest_waiting_time() {
    let mut row = active_row();
    let mut behind = kind("widgets");
    behind.waiting = 3;
    behind.behind = true;
    behind.oldest_waiting_ready_at = Some(Utc::now());

    apply_kind_problems(&mut row, &[behind]);

    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("oldest waiting since"));
}
