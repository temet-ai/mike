use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::followups::{
    Followups, analyze, anchors, file_type, hidden_text, parse_cut, pick_sources,
};
use super::test_support::{arms, at, call, cut, source, who};
use super::types::{Gaps, stamp};

const SUPERSEDED: &str = "file a.py: a later whole read";

fn numbered(count: usize) -> Value {
    Value::String(
        (1..=count)
            .map(|n| format!("{n:>6}\tline {n}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[test]
fn hidden_text_numbered_lines_outside_the_kept_ranges_are_hidden() -> Result<()> {
    let kept = Some(vec![(1, 2), (9, 10)]);
    let (hidden, shown) = hidden_text(&numbered(10), &kept).context("text")?;
    assert!(hidden.contains("line 5"));
    assert!(!format!("{hidden}\n").contains("line 1\n"));
    assert!(shown.contains("line 9"));
    Ok(())
}

#[test]
fn hidden_text_superseded_read_hides_everything() -> Result<()> {
    let content = json!("     1\ta\n     2\tb");
    let (hidden, shown) = hidden_text(&content, &None).context("text")?;
    assert_eq!(hidden, "a\nb");
    assert_eq!(shown, "");
    Ok(())
}

#[test]
fn hidden_text_joins_text_parts_and_rejects_other_content() -> Result<()> {
    let parts = json!([{"type": "text", "text": "x\n"}, {"type": "text", "text": "y"}]);
    let (hidden, _) = hidden_text(&parts, &None).context("text")?;
    assert_eq!(hidden, "x\ny");
    assert!(hidden_text(&Value::Null, &None).is_none());
    Ok(())
}

#[test]
fn parse_cut_reads_the_kept_ranges_or_the_superseded_marker() {
    let reduced = cut(
        who(1, None, None, "primary"),
        "w",
        "file_read_reduced",
        "file a.rs: kept file lines 1-20 and 90-100 (40 of 100 lines)",
        "2026-10-18T10:00:00Z",
    );
    assert_eq!(
        parse_cut(&reduced),
        Some(("a.rs".to_string(), Some(vec![(1, 20), (90, 100)])))
    );
    let superseded = cut(
        who(1, None, None, "primary"),
        "w",
        "file_read_superseded",
        SUPERSEDED,
        "2026-10-18T10:00:00Z",
    );
    assert_eq!(parse_cut(&superseded), Some(("a.py".to_string(), None)));
    let unparsed = cut(
        who(1, None, None, "primary"),
        "w",
        "file_read_reduced",
        SUPERSEDED,
        "2026-10-18T10:00:00Z",
    );
    assert_eq!(parse_cut(&unparsed), None);
}

#[test]
fn file_type_is_the_lowercased_suffix_or_none() {
    assert_eq!(file_type("src/Main.RS"), ".rs");
    assert_eq!(file_type("Makefile"), "(none)");
    assert_eq!(file_type(".bashrc"), "(none)");
    assert_eq!(file_type("dir.d/file"), "(none)");
    assert_eq!(file_type("notes."), "(none)");
}

#[test]
fn anchors_come_from_edit_and_multiedit_inputs() {
    let mut edit = call(
        who(1, None, None, "primary"),
        1,
        "Edit",
        Some("a"),
        "2026-10-18T10:00:00Z",
    );
    edit.input = Some(json!({"old_string": "one"}));
    assert_eq!(anchors(&edit), vec!["one".to_string()]);
    let mut multi = edit.clone();
    multi.tool_name = "MultiEdit".to_string();
    multi.input = Some(json!({"edits": [{"old_string": "a"}, {"old_string": "b"}]}));
    assert_eq!(anchors(&multi), vec!["a".to_string(), "b".to_string()]);
}

fn worker() -> super::types::Who {
    who(1, Some(2), Some("worker"), "subagent")
}

fn superseded_cut(wire: &str) -> super::types::Cut {
    cut(
        worker(),
        wire,
        "file_read_superseded",
        SUPERSEDED,
        "2026-10-18T10:00:01Z",
    )
}

fn hidden_source(id: i64, wire: &str) -> super::types::Source {
    source(
        worker(),
        id,
        wire,
        Some("a.py"),
        json!("hidden"),
        "2026-10-18T10:00:00Z",
    )
}

#[test]
fn analyze_counts_lane_follow_ups_once_and_drops_censored_windows() {
    let sources = pick_sources(vec![hidden_source(10, "read")]);
    let reread = call(worker(), 11, "Read", Some("a.py"), "2026-10-18T10:00:02Z");
    let foreign = call(
        who(1, Some(3), Some("worker"), "subagent"),
        12,
        "Read",
        Some("a.py"),
        "2026-10-18T10:00:02Z",
    );
    let assigned = arms(&[(1, "forward")]);
    let cuts = [superseded_cut("read"), superseded_cut("read")];
    let calls = [reread, foreign];
    let until = stamp(at("2026-10-18T11:00:00Z"));
    let input = Followups {
        cuts: &cuts,
        arms: &assigned,
        calls: &calls,
        sources: &sources,
        until,
        window: 1800.0,
    };
    let (table, gaps) = analyze(&input);
    let row = &table[&(".py".to_string(), "forward".to_string())];
    assert_eq!((row.cuts, row.reread), (1, 1));
    assert!(gaps.is_empty());
    let censored = Followups {
        until: until - 3590.0,
        ..input
    };
    let (table, gaps) = analyze(&censored);
    assert!(table.is_empty());
    assert_eq!(gaps, Gaps::from([("right_censored".to_string(), 1)]));
}

#[test]
fn analyze_overlapping_sources_share_a_follow_up_and_unknown_lanes_are_gaps() {
    let sources = pick_sources(vec![hidden_source(10, "one"), hidden_source(11, "two")]);
    let unknown = cut(
        who(1, None, None, "subagent"),
        "three",
        "file_read_superseded",
        SUPERSEDED,
        "2026-10-18T10:00:01Z",
    );
    let cuts = [superseded_cut("one"), superseded_cut("two"), unknown];
    let calls = [call(
        worker(),
        12,
        "Read",
        Some("a.py"),
        "2026-10-18T10:00:05Z",
    )];
    let assigned = arms(&[(1, "forward")]);
    let (table, gaps) = analyze(&Followups {
        cuts: &cuts,
        arms: &assigned,
        calls: &calls,
        sources: &sources,
        until: stamp(at("2026-10-18T11:00:00Z")),
        window: 1800.0,
    });
    let row = &table[&(".py".to_string(), "forward".to_string())];
    assert_eq!((row.cuts, row.reread, row.exposures_with_reread), (2, 1, 2));
    assert_eq!(
        gaps,
        Gaps::from([("missing_agent_identity".to_string(), 1)])
    );
}

#[test]
fn analyze_counts_an_edit_whose_anchor_sits_only_in_the_hidden_lines() {
    let content = numbered(10);
    let sources = pick_sources(vec![source(
        worker(),
        10,
        "read",
        Some("a.rs"),
        content,
        "2026-10-18T10:00:00Z",
    )]);
    let reduced = cut(
        worker(),
        "read",
        "file_read_reduced",
        "file a.rs: kept file lines 1-2 and 9-10 (4 of 10 lines)",
        "2026-10-18T10:00:01Z",
    );
    let mut hidden_edit = call(worker(), 11, "Edit", Some("a.rs"), "2026-10-18T10:00:05Z");
    hidden_edit.input = Some(json!({"old_string": "line 5"}));
    let mut shown_edit = call(worker(), 12, "Edit", Some("a.rs"), "2026-10-18T10:00:06Z");
    shown_edit.input = Some(json!({"old_string": "line 9"}));
    let assigned = arms(&[(1, "control")]);
    let (table, _) = analyze(&Followups {
        cuts: &[reduced],
        arms: &assigned,
        calls: &[hidden_edit, shown_edit],
        sources: &sources,
        until: stamp(at("2026-10-18T11:00:00Z")),
        window: 1800.0,
    });
    let row = &table[&(".rs".to_string(), "control".to_string())];
    assert_eq!(
        (row.cuts, row.edit_hidden, row.exposures_with_hidden_edit),
        (1, 1, 1)
    );
}

#[test]
fn analyze_source_for_another_path_is_a_gap() {
    let sources = pick_sources(vec![source(
        worker(),
        10,
        "read",
        Some("other.py"),
        json!("hidden"),
        "2026-10-18T10:00:00Z",
    )]);
    let assigned = arms(&[(1, "forward")]);
    let (table, gaps) = analyze(&Followups {
        cuts: &[superseded_cut("read")],
        arms: &assigned,
        calls: &[],
        sources: &sources,
        until: stamp(at("2026-10-18T11:00:00Z")),
        window: 1800.0,
    });
    assert!(table.is_empty());
    assert_eq!(gaps, Gaps::from([("source_path_mismatch".to_string(), 1)]));
}

#[test]
fn lane_is_unknown_without_agent_identity_off_the_primary_origin() {
    assert!(who(1, None, None, "subagent").lane().is_none());
    assert!(who(1, None, None, "primary").lane().is_some());
    assert!(who(1, None, None, "primary_first").lane().is_some());
    assert!(who(1, None, Some("a"), "subagent").lane().is_some());
}

#[test]
fn pick_sources_keeps_the_earliest_receipt_and_drops_ambiguous_wire_ids() -> Result<()> {
    let early = source(
        worker(),
        10,
        "w",
        Some("a.py"),
        json!("x"),
        "2026-10-18T10:00:00Z",
    );
    let late = source(
        worker(),
        10,
        "w",
        Some("a.py"),
        json!("x"),
        "2026-10-18T10:05:00Z",
    );
    let other = source(
        worker(),
        99,
        "w",
        Some("a.py"),
        json!("x"),
        "2026-10-18T10:06:00Z",
    );
    let kept = pick_sources(vec![late.clone(), early.clone()]);
    let held = kept.values().next().context("one source")?;
    assert_eq!(held.result_at, early.result_at);
    assert!(pick_sources(vec![early, late, other]).is_empty());
    Ok(())
}
