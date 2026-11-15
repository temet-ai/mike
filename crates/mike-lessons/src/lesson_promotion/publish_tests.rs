use super::super::ledger::LedgerEntry;
use super::super::test_support::{key, project_uuid};
use super::*;

fn entry(first: &str) -> LedgerEntry {
    LedgerEntry {
        first_emitted: first.to_owned(),
        ..LedgerEntry::default()
    }
}

#[test]
fn installs_carry_retired_entries_and_skip_entries_that_were_never_written() {
    let mut written = entry("2026-10-01");
    written.path = Some("/p/a/SKILL.md".to_owned());
    written.draft_id = Some(3);
    written.lesson_watermark = 9;
    written.retired_at = Some("2026-10-10".to_owned());
    written.retired_reason = Some("user".to_owned());
    let mut old = entry("2026-09-01");
    old.lesson_watermark = 1;
    let ledger: Ledger = [(key("a"), written), (key("old"), old)]
        .into_iter()
        .collect();
    let found = installs(&ledger);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].project_uuid, project_uuid());
    assert_eq!(found[0].folder, "a");
    assert_eq!(found[0].draft_id, Some(3));
    assert_eq!(found[0].lesson_watermark, 9);
    assert_eq!(found[0].retired_reason.as_deref(), Some("user"));
    assert_eq!(
        found[0].retired_at.map(|day| day.to_string()),
        Some("2026-10-10".to_owned())
    );
}

#[test]
fn a_reason_without_a_retirement_day_is_not_reported() {
    let mut written = entry("2026-10-01");
    written.path = Some("/p/a/SKILL.md".to_owned());
    written.retired_reason = Some("user".to_owned());
    let ledger: Ledger = [(key("a"), written)].into_iter().collect();
    assert!(installs(&ledger)[0].retired_reason.is_none());
}

#[test]
fn waiting_outcomes_that_do_not_parse_or_disagree_with_themselves_are_named() {
    let mut broken = entry("2026-10-01");
    broken.outcome = Some(serde_json::json!({ "folder": "one" }));
    let mut inconsistent = entry("2026-10-01");
    inconsistent.outcome = Some(serde_json::json!({
        "project": project_uuid(),
        "folder": "deploy",
        "first_shipped_on": "2026-10-01",
        "window_start": "2026-10-01T00:00:00Z",
        "window_end": "2026-10-02T00:00:00Z",
        "loads": 12,
        "load_sessions": 4,
        "loaded_turns": 9,
        "judged_turns": 7,
        "corrected_turns": 8,
        "base_judged_turns": 40,
        "base_corrected_turns": 8
    }));
    let mut fine = inconsistent.clone();
    if let Some(value) = fine.outcome.as_mut() {
        value["corrected_turns"] = serde_json::json!(3);
    }
    let ledger: Ledger = [
        ("a:broken".to_owned(), broken),
        ("a:inconsistent".to_owned(), inconsistent),
        ("a:fine".to_owned(), fine),
    ]
    .into_iter()
    .collect();
    let (waiting, problems) = parse_waiting(&ledger);
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].key, "a:fine");
    assert_eq!(problems.len(), 2);
    assert!(problems.iter().any(|p| p.starts_with("a:broken")));
    assert!(
        problems
            .iter()
            .any(|p| p.starts_with("a:inconsistent") && p.contains("corrected_turns is 8"))
    );
}
