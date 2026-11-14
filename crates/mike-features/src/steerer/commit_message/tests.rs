use std::collections::BTreeMap;

use super::compose::compose;
use super::git::{belongs_to_command, is_commit_command, parse_head};
use super::questions::{DESCRIBES, UNMENTIONED};
use super::state::{Commit, state};
use crate::llm::decisions::Answer;
use crate::llm::decisions::point::Asked;

fn answered(describes: f64, unmentioned: f64) -> Asked {
    let noul = |p| Answer::Noul {
        noul: p,
        confidence: None,
    };
    Asked::answered(BTreeMap::from([
        (DESCRIBES.to_owned(), noul(describes)),
        (UNMENTIONED.to_owned(), noul(unmentioned)),
    ]))
}

fn commit() -> Commit {
    Commit {
        sha: "0123456789abcdef".to_owned(),
        subject: "Fix the clearance".to_owned(),
        message: "Fix the clearance\n\nBody".to_owned(),
        files: vec!["a.rs".to_owned(), "b.rs".to_owned()],
        diff: "+x".repeat(10_000),
    }
}

#[test]
fn compose_a_message_that_matches_the_diff_does_not_warn() {
    assert_eq!(compose(&answered(0.9, 0.1)).map(|w| w.warn), Some(false));
}

#[test]
fn compose_a_message_that_misses_the_diff_warns_with_a_mismatch_note() {
    let w = compose(&answered(0.1, 0.1));
    assert_eq!(w.as_ref().map(|w| (w.warn, w.mismatch)), Some((true, true)));
    let note = w.map(|w| w.note(&commit())).unwrap_or_default();
    assert!(note.contains("0123456") && note.contains("a.rs, b.rs"));
}

#[test]
fn compose_unmentioned_work_warns_without_a_mismatch() {
    let w = compose(&answered(0.9, 0.8));
    assert_eq!(w.map(|w| (w.warn, w.mismatch)), Some((true, false)));
}

#[test]
fn compose_failed_ask_is_none_so_nothing_is_said() {
    assert!(compose(&Asked::failed_with("timeout")).is_none());
}

#[test]
fn state_caps_the_diff_and_reports_the_file_count() {
    let s = state(&commit());
    assert_eq!(s["diff"].as_str().map(str::len), Some(7_000));
    assert_eq!(s["files_changed_total"], 2);
}

#[test]
fn parse_head_reads_sha_time_and_message() {
    let parsed = parse_head("abc123\u{0}1700000000\u{0}Fix it\n\nbody\n");
    assert_eq!(
        parsed,
        Some((
            "abc123".to_owned(),
            "Fix it\n\nbody".to_owned(),
            1_700_000_000
        ))
    );
    assert!(parse_head("garbage").is_none());
}

#[test]
fn belongs_to_command_needs_the_subject_in_the_command_and_a_fresh_commit() {
    let command = "git commit -m \"Fix it\"";
    assert!(belongs_to_command(command, "Fix it\n\nbody", 1_000, 1_100));
    assert!(!belongs_to_command(command, "Other subject", 1_000, 1_100));
    assert!(!belongs_to_command(command, "Fix it", 1_000, 5_000));
}

#[test]
fn is_commit_command_matches_only_bash_git_commit() {
    assert!(is_commit_command("Bash", "git add . && git commit -m x"));
    assert!(!is_commit_command("Read", "git commit"));
    assert!(!is_commit_command("Bash", "git status"));
}
