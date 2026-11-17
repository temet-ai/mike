#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use super::*;
use crate::launch::brief::DEFAULT_BRIEF_INLINE_MAX;

fn spec(tokens: &[&str], prompt: Option<&str>) -> LaunchSpec {
    let dir = tempfile::tempdir().unwrap();
    let mut tokens: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
    tokens.insert(0, "--no-channels".to_string());
    let split = split_launch_args(tokens).unwrap();
    let opts = AssembleOptions {
        program: "claude".into(),
        proxy: None,
        brief_inline_max: DEFAULT_BRIEF_INLINE_MAX,
        briefs_dir: dir.path(),
        brief_notes: false,
        brief_by_mail: false,
    };
    LaunchSpec::assemble(split, prompt.map(str::to_string), &opts).unwrap()
}

#[test]
fn session_name_becomes_claude_name() {
    let spec = spec(
        &["--session-name", "lander", "--no-remote-control"],
        Some("brief"),
    );
    let id = spec.session.unwrap().to_string();
    assert_eq!(
        spec.args,
        ["--session-id", &id, "--name", "lander", "brief"]
    );
}

#[test]
fn caller_name_wins_over_session_name() {
    let spec = spec(
        &[
            "--session-name",
            "lander",
            "--name=mine",
            "--no-remote-control",
        ],
        None,
    );
    assert!(!spec.args.iter().any(|a| a == "--name"));
}
