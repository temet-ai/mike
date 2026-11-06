use chrono::{NaiveDate, TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

use super::render;
use crate::store::persistence::work_threads::{
    HistoryEntry, LinkedSession, ThreadDetail, ThreadIdentity, ThreadState,
};

fn instant(day: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, 9, 30, 0)
        .single()
        .unwrap_or_default()
}

fn entry(ord: i32, kind: &str, summary: &str, body: &str) -> HistoryEntry {
    HistoryEntry {
        ord,
        kind: kind.to_owned(),
        entry_date: NaiveDate::from_ymd_opt(2026, 10, 20).unwrap_or_default(),
        session_uuid: None,
        commit_sha: None,
        summary: summary.to_owned(),
        body: body.to_owned(),
        data: Some(json!({})),
        created_at: instant(20),
    }
}

fn detail() -> ThreadDetail {
    ThreadDetail {
        thread: ThreadIdentity {
            id: 1,
            slug: "export-me".to_owned(),
            title: Some("Export me".to_owned()),
            goal: Some("Show everything.".to_owned()),
            status: Some("review".to_owned()),
            branch: Some("feat/export".to_owned()),
            worktree: None,
            last_session_uuid: None,
            source_path: None,
            parse_error: None,
            created_at: instant(18),
            updated_at: instant(20),
        },
        state: Some(ThreadState {
            tldr: "Short version".to_owned(),
            where_we_are: "Half done".to_owned(),
            next_up: "Finish".to_owned(),
            blockers: String::new(),
            open_questions: String::new(),
            decisions: "Use Markdown".to_owned(),
            files: vec!["docs/x.md".to_owned()],
            updated_at: instant(20),
        }),
        acceptance_criteria: Some("A1. It renders.".to_owned()),
        depends_on: vec!["base".to_owned()],
        depended_on_by: Vec::new(),
        history: vec![
            entry(2, "handoff", "Pick up here", "Pick up here\nthen test it"),
            entry(1, "checkpoint", "Short version", "Short version"),
        ],
        history_total: 2,
        history_next_before_ord: None,
        sessions: vec![LinkedSession {
            session_uuid: Uuid::nil(),
            contribution: "wrote it".to_owned(),
            first_seen_at: instant(18),
            last_seen_at: instant(20),
        }],
    }
}

#[test]
fn a_thread_renders_every_part_with_the_history_newest_first() {
    let text = render(&detail());
    assert!(text.starts_with("# Export me\n"));
    for expected in [
        "- Status: review",
        "- Branch: feat/export",
        "## Goal\n\nShow everything.",
        "### Where we are\n\nHalf done",
        "### Decisions\n\nUse Markdown",
        "- `docs/x.md`",
        "## Acceptance criteria\n\nA1. It renders.",
        "## Depends on\n\n`base`",
        "`00000000-0000-0000-0000-000000000000` wrote it",
        "### 2026-10-20 handoff (entry 2): Pick up here\n\nPick up here\nthen test it",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
    assert!(!text.contains("### Blockers"));
    assert!(!text.contains("## Needed by"));
    let handoff = text.find("entry 2");
    let checkpoint = text.find("entry 1");
    assert!(handoff < checkpoint, "history must run newest first");
}

#[test]
fn a_thread_without_a_title_is_headed_by_its_slug_and_shows_its_parse_error() {
    let mut bare = detail();
    bare.thread.title = None;
    bare.thread.parse_error = Some("no title".to_owned());
    bare.state = None;
    bare.acceptance_criteria = None;
    let text = render(&bare);
    assert!(text.starts_with("# export-me\n"));
    assert!(text.contains("- Parse error: no title"));
    assert!(!text.contains("## State of play"));
}
