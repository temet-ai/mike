#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::path::Path;

use uuid::Uuid;

use super::*;
use crate::launch::brief::{BriefMode, DEFAULT_BRIEF_INLINE_MAX};

fn toks(s: &[&str]) -> Vec<String> {
    s.iter().map(|t| t.to_string()).collect()
}

fn spec_with(dir: &Path, tokens: &[&str], prompt: Option<&str>, notes: bool) -> LaunchSpec {
    let mut tokens = toks(tokens);
    tokens.insert(0, "--no-channels".to_string());
    let split = split_launch_args(tokens).unwrap();
    let opts = AssembleOptions {
        program: "claude".into(),
        proxy: None,
        brief_inline_max: DEFAULT_BRIEF_INLINE_MAX,
        briefs_dir: dir,
        brief_notes: notes,
        brief_by_mail: notes,
    };
    LaunchSpec::assemble(split, prompt.map(str::to_string), &opts).unwrap()
}

#[test]
fn resume_with_a_uuid_knows_its_session_without_preassigning_one() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let spec = spec_with(
        dir.path(),
        &["--resume", &id.to_string(), "--no-remote-control"],
        None,
        true,
    );
    assert_eq!(spec.session, Some(id));
    assert!(!spec.preassigned);
    assert_eq!(spec.args, toks(&["--resume", &id.to_string()]));
}

#[test]
fn resume_equals_form_with_a_uuid_knows_its_session() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let spec = spec_with(dir.path(), &[&format!("--resume={id}")], None, true);
    assert_eq!(spec.session, Some(id));
    assert!(!spec.preassigned);
}

#[test]
fn resume_that_forks_or_continues_or_names_no_uuid_stays_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4().to_string();
    for tokens in [
        vec!["--resume", id.as_str(), "--fork-session"],
        vec!["--continue"],
        vec!["--resume", "search words"],
        vec!["--resume"],
    ] {
        let spec = spec_with(dir.path(), &tokens, None, true);
        assert_eq!(spec.session, None, "{tokens:?}");
        assert!(!spec.preassigned, "{tokens:?}");
    }
}

#[test]
fn long_brief_with_a_ledger_is_a_note_brief_kept_off_the_argv() {
    let dir = tempfile::tempdir().unwrap();
    let long = "b".repeat(1200);
    let spec = spec_with(dir.path(), &[], Some(&long), true);
    let brief = spec.brief.clone().unwrap();
    assert_eq!(brief.mode, BriefMode::Note);
    let note = brief.note.unwrap();
    assert!(
        !spec.args.contains(&brief.delivered),
        "a note brief is pushed over Remote Control, not typed"
    );
    assert!(brief.delivered.contains(&format!("message_uuid={note}")));
    assert!(brief.delivered.contains(&spec.session.unwrap().to_string()));
    assert_eq!(spec.prompt.as_deref(), Some(long.as_str()));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn resumed_session_gets_a_note_brief_under_its_own_id() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let long = "r".repeat(900);
    let spec = spec_with(
        dir.path(),
        &["--resume", &id.to_string()],
        Some(&long),
        true,
    );
    let brief = spec.brief.unwrap();
    assert_eq!(brief.mode, BriefMode::Note);
    assert!(brief.delivered.contains(&format!("session_id={id}")));
}

#[test]
fn continue_with_a_long_brief_falls_back_to_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let spec = spec_with(dir.path(), &["--continue"], Some(&"c".repeat(900)), true);
    let brief = spec.brief.unwrap();
    assert_eq!(brief.mode, BriefMode::File);
    assert_eq!(brief.note, None);
}

#[test]
fn demote_brief_to_file_puts_the_file_pointer_on_the_argv() {
    let dir = tempfile::tempdir().unwrap();
    let long = "d".repeat(900);
    let mut spec = spec_with(dir.path(), &[], Some(&long), true);
    let before = spec.args.len();
    spec.demote_brief_to_file(dir.path()).unwrap();
    let brief = spec.brief.clone().unwrap();
    assert_eq!(brief.mode, BriefMode::File);
    assert_eq!(brief.note, None);
    assert_eq!(spec.args.len(), before + 2);
    assert_eq!(spec.args.last().unwrap(), &brief.delivered);
    assert_eq!(
        spec.args[spec.args.len() - 2],
        format!(
            "--allowedTools=Read(/{})",
            brief.path.clone().unwrap().display()
        )
    );
    assert!(!spec.args.last().unwrap().contains("message_uuid="));
    assert_eq!(std::fs::read_to_string(brief.path.unwrap()).unwrap(), long);
}

#[test]
fn a_file_brief_launch_allows_the_seat_to_read_that_file_whatever_its_permission_mode() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = ["--permission-mode", "dontAsk", "--allowedTools", "Bash"];
    let spec = spec_with(dir.path(), &tokens, Some(&"z".repeat(801)), false);
    let path = spec.brief.unwrap().path.unwrap();
    let n = spec.args.len();
    assert_eq!(
        spec.args[n - 2],
        format!("--allowedTools=Read(/{})", path.display())
    );
    assert!(spec.args[n - 2].starts_with("--allowedTools=Read(//"));
    assert!(spec.args[n - 1].starts_with("Your brief is saved at "));
}

#[test]
fn an_inline_brief_launch_adds_no_read_rule() {
    let dir = tempfile::tempdir().unwrap();
    let spec = spec_with(dir.path(), &[], Some("short"), false);
    assert!(!spec.args.iter().any(|a| a.starts_with("--allowedTools")));
}
