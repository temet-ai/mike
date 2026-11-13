use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind};
use notify::{Event, EventKind};

use super::super::author::{Authorship, decide, from_shell_runs};
use super::super::notifications::{Places, Signal, signals};
use super::super::refs::seat_uuid;
use super::super::supervisor::{backoff, plan};
use super::super::tree::{DeltaKind, differences};
use crate::change_record::{ChangeKind, ShellRun, ToolCallNaming};

fn run(session_id: i64) -> ShellRun {
    ShellRun {
        session_id,
        started: Utc::now(),
        finished: None,
    }
}

#[test]
fn one_running_shell_names_its_session() {
    assert_eq!(
        from_shell_runs(&[run(7), run(7)]),
        Authorship::Shell {
            session: 7,
            named_by: None
        }
    );
}

#[test]
fn two_overlapping_shells_name_no_author_and_list_both() {
    let who = from_shell_runs(&[run(9), run(4)]);
    assert_eq!(
        who,
        Authorship::Unknown {
            candidates: vec![4, 9]
        }
    );
    assert_eq!(who.session(), None);
    assert_eq!(who.file_kind(), ChangeKind::ShellChange);
    assert_eq!(who.detail()["candidates"], serde_json::json!([4, 9]));
}

#[test]
fn no_running_shell_names_no_author() {
    assert_eq!(
        from_shell_runs(&[]),
        Authorship::Unknown { candidates: vec![] }
    );
}

fn call(session_id: i64, tool_use_id: i64, secs: i64) -> ToolCallNaming {
    ToolCallNaming {
        session_id,
        tool_use_id,
        tool_name: "Edit".into(),
        at: Utc::now() - chrono::Duration::seconds(secs),
    }
}

#[test]
fn one_session_editing_names_its_newest_call() {
    let who = decide(&[call(2, 10, 20), call(2, 11, 3)], &[run(2)]);
    assert_eq!(
        who,
        Authorship::Edit {
            session: 2,
            tool_use_id: 11,
            tool_name: "Edit".into(),
            content_verified: false
        }
    );
    assert_eq!(who.detail()["content_verified"], false);
    assert_eq!(
        decide(&[], &[run(5)]),
        Authorship::Shell {
            session: 5,
            named_by: None
        }
    );
}

#[test]
fn two_sessions_editing_the_path_name_no_author() {
    let who = decide(&[call(1, 10, 20), call(2, 11, 3)], &[]);
    assert_eq!(
        who,
        Authorship::Unknown {
            candidates: vec![1, 2]
        }
    );
    assert_eq!(who.file_kind(), ChangeKind::ShellChange);
}

#[test]
fn an_edit_with_another_sessions_shell_running_names_no_author() {
    let who = decide(&[call(1, 10, 3)], &[run(8)]);
    assert_eq!(
        who,
        Authorship::Unknown {
            candidates: vec![1, 8]
        }
    );
}

#[test]
fn the_plan_starts_new_checkouts_and_stops_quiet_ones() {
    let running: BTreeSet<String> = ["/a", "/b"].into_iter().map(String::from).collect();
    let wanted: BTreeMap<String, i64> = [("/b".to_string(), 2), ("/c".to_string(), 3)].into();
    let (begin, end) = plan(&running, &wanted);
    assert_eq!(begin, vec![("/c".to_string(), 3)]);
    assert_eq!(end, vec!["/a".to_string()]);
}

#[test]
fn failures_back_off_up_to_ten_minutes() {
    assert_eq!(backoff(1), Duration::from_secs(60));
    assert_eq!(backoff(2), Duration::from_secs(120));
    assert_eq!(backoff(40), Duration::from_secs(600));
}

fn places() -> Places {
    Places {
        root: PathBuf::from("/w"),
        git_dir: PathBuf::from("/w/.git"),
        common_dir: PathBuf::from("/w/.git"),
    }
}

fn event(kind: EventKind, path: &str) -> notify::Result<Event> {
    Ok(Event::new(kind).add_path(PathBuf::from(path)))
}

#[test]
fn a_write_in_the_tree_is_a_path_and_a_ref_write_is_a_ref() {
    let write = EventKind::Modify(ModifyKind::Any);
    assert_eq!(
        signals(&places(), event(write, "/w/src/a.rs")),
        vec![Signal::Path("src/a.rs".into())]
    );
    assert_eq!(
        signals(&places(), event(write, "/w/.git/refs/heads/main")),
        vec![Signal::Refs]
    );
    assert_eq!(
        signals(&places(), event(write, "/w/.git/packed-refs")),
        vec![Signal::Refs]
    );
}

#[test]
fn git_objects_the_index_and_reads_are_left_out() {
    let create = EventKind::Create(CreateKind::File);
    assert!(signals(&places(), event(create, "/w/.git/objects/ab/cdef")).is_empty());
    assert!(signals(&places(), event(create, "/w/.git/index")).is_empty());
    let open = EventKind::Access(AccessKind::Open(AccessMode::Any));
    assert!(signals(&places(), event(open, "/w/src/a.rs")).is_empty());
    let closed = EventKind::Access(AccessKind::Close(AccessMode::Write));
    assert_eq!(signals(&places(), event(closed, "/w/src/a.rs")).len(), 1);
    assert!(signals(&places(), event(create, "/elsewhere/x")).is_empty());
}

#[test]
fn a_notification_error_asks_for_a_rescan() {
    let failed = Err(notify::Error::generic("queue overflow"));
    assert_eq!(signals(&places(), failed), vec![Signal::Rescan]);
}

#[test]
fn the_fleet_seat_trailer_gives_the_session() {
    let id = "6c8c2d1e-0000-4000-8000-000000000001";
    assert_eq!(
        seat_uuid(&format!("rust-T2 ({id})\n")).map(|u| u.to_string()),
        Some(id.to_string())
    );
    assert_eq!(seat_uuid("no session here"), None);
    assert_eq!(seat_uuid(""), None);
}

#[test]
fn differences_carry_the_content_before_and_after() {
    let before: BTreeMap<String, String> = [("a", "1"), ("b", "2")]
        .into_iter()
        .map(|(p, s)| (p.into(), s.into()))
        .collect();
    let after: BTreeMap<String, String> = [("a", "9"), ("c", "3")]
        .into_iter()
        .map(|(p, s)| (p.into(), s.into()))
        .collect();
    let found = differences(&before, &after);
    let shown: Vec<(&str, DeltaKind, Option<&str>, Option<&str>)> = found
        .iter()
        .map(|d| {
            (
                d.path.as_str(),
                d.change,
                d.before_sha.as_deref(),
                d.after_sha.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        vec![
            ("a", DeltaKind::Modified, Some("1"), Some("9")),
            ("c", DeltaKind::Added, None, Some("3")),
            ("b", DeltaKind::Deleted, Some("2"), None),
        ]
    );
}
