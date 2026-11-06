#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::path::Path;

use uuid::Uuid;

use super::claude_args::{bare_prompt_present, slug, title};
use super::*;
use crate::launch::backend::{Placement, SplitDirection};
use crate::launch::brief::{BriefMode, DEFAULT_BRIEF_INLINE_MAX, PromptSource};

fn toks(s: &[&str]) -> Vec<String> {
    s.iter().map(|t| t.to_string()).collect()
}

fn opts<'a>(dir: &'a Path, proxy: Option<&'a ProxyEnv>) -> AssembleOptions<'a> {
    AssembleOptions {
        program: "claude".into(),
        proxy,
        brief_inline_max: DEFAULT_BRIEF_INLINE_MAX,
        briefs_dir: dir,
        brief_notes: false,
        brief_by_mail: false,
    }
}

fn assemble(tokens: &[&str], prompt: Option<&str>) -> LaunchSpec {
    let dir = tempfile::tempdir().unwrap();
    let mut with_tokens = toks(tokens);
    if !with_tokens
        .iter()
        .any(|t| t == "--no-channels" || t.starts_with("--dangerously-load-development-channels"))
    {
        with_tokens.insert(0, "--no-channels".to_string());
    }
    let split = split_launch_args(with_tokens).unwrap();
    LaunchSpec::assemble(split, prompt.map(str::to_string), &opts(dir.path(), None)).unwrap()
}

#[test]
fn split_resume_fork_session_keeps_nothing_and_preassigns_nothing() {
    let spec = assemble(
        &[
            "--resume",
            "abc",
            "--fork-session",
            "hi",
            "--no-remote-control",
        ],
        None,
    );
    assert_eq!(
        spec.args,
        toks(&["--resume", "abc", "--fork-session", "hi"])
    );
    assert_eq!(spec.session, None);
    assert!(!spec.preassigned);
    assert_eq!(spec.flags.placement, None);
}

#[test]
fn split_model_opus_tab_hi_preassigns_first_and_keeps_tab() {
    let spec = assemble(
        &["--model", "opus", "--tab", "hi", "--no-remote-control"],
        None,
    );
    let id = spec.session.unwrap();
    assert!(spec.preassigned);
    assert_eq!(
        spec.args,
        toks(&["--session-id", &id.to_string(), "--model", "opus", "hi"])
    );
    assert_eq!(spec.flags.placement, Some(Placement::Tab));
    assert_eq!(spec.title, "cc·opus");
    assert_eq!(spec.model.as_deref(), Some("opus"));
}

#[test]
fn split_prompt_file_agent_w_appends_contents_last() {
    let split = split_launch_args(toks(&["--prompt-file", "b.md", "--agent", "w"])).unwrap();
    assert_eq!(split.prompt_file, Some(PromptSource::File("b.md".into())));
    let dir = tempfile::tempdir().unwrap();
    let spec =
        LaunchSpec::assemble(split, Some("contents".into()), &opts(dir.path(), None)).unwrap();
    let id = spec.session.unwrap().to_string();
    assert_eq!(
        spec.args,
        toks(&[
            "--session-id",
            &id,
            "--agent",
            "w",
            "--remote-control",
            "--dangerously-load-development-channels",
            "server:mike",
            "contents"
        ])
    );
    assert_eq!(spec.agent.as_deref(), Some("w"));
    assert_eq!(spec.prompt.as_deref(), Some("contents"));
    assert_eq!(spec.brief.as_ref().unwrap().mode, BriefMode::Inline);
    assert_eq!(spec.title, "cc — contents");
}

#[test]
fn split_double_dash_forwards_spawn_looking_flags_verbatim() {
    let split = split_launch_args(toks(&["--", "--append-system-prompt", "--tab"])).unwrap();
    assert_eq!(
        split.claude_args,
        toks(&["--append-system-prompt", "--tab"])
    );
    assert_eq!(split.placement, None);
    let spec = assemble(
        &[
            "--",
            "--append-system-prompt",
            "--tab",
            "--no-remote-control",
        ],
        None,
    );
    let id = spec.session.unwrap().to_string();
    assert_eq!(
        spec.args,
        toks(&[
            "--session-id",
            &id,
            "--append-system-prompt",
            "--tab",
            "--no-remote-control",
            "--remote-control"
        ])
    );
}

#[test]
fn split_fork_style_resume_with_bare_prompt_words_stays_in_order() {
    let split = split_launch_args(toks(&["--resume", "t", "--fork-session", "fix it"])).unwrap();
    assert_eq!(
        split.claude_args,
        toks(&["--resume", "t", "--fork-session", "fix it"])
    );
}

#[test]
fn split_legacy_m_and_p_are_refused_with_the_reason() {
    let err = split_launch_args(toks(&["-m", "opus"])).unwrap_err();
    assert!(err.to_string().contains("legacy `-m"), "{err}");
    let err = split_launch_args(toks(&["-p", "fix"])).unwrap_err();
    assert!(err.to_string().contains("print mode"), "{err}");
    assert_eq!(err.exit_code(), 1);
}

#[test]
fn split_spawn_flags_anywhere_and_value_forms() {
    let split = split_launch_args(toks(&[
        "--model",
        "opus",
        "--json",
        "--split=r",
        "--run",
        "r1",
        "--seat=obs",
        "--trust",
        "--no-proxy-env",
        "--worktree-ok",
        "--no-carry-config",
        "--anchor-terminal",
        "t9",
        "hi",
    ]))
    .unwrap();
    assert!(split.json && split.trust && split.worktree_ok);
    assert!(!split.proxy_env && !split.carry_config);
    assert_eq!(
        split.placement,
        Some(Placement::Split(SplitDirection::Right))
    );
    assert_eq!(split.run.as_deref(), Some("r1"));
    assert_eq!(split.seat.as_deref(), Some("obs"));
    assert_eq!(split.anchor_terminal.as_deref(), Some("t9"));
    assert_eq!(split.claude_args, toks(&["--model", "opus", "hi"]));
}

#[test]
fn split_identity_flags_parse_and_leave_claude_args_alone() {
    let split = split_launch_args(toks(&[
        "--role",
        "observer",
        "--instructions",
        "watch the fleet for rot",
        "--goal",
        "ship phase 1",
        "--session-name",
        "observer-1",
        "hi",
    ]))
    .unwrap();
    assert_eq!(split.role.as_deref(), Some("observer"));
    assert_eq!(
        split.instructions.as_deref(),
        Some("watch the fleet for rot")
    );
    assert_eq!(split.goal.as_deref(), Some("ship phase 1"));
    assert_eq!(split.session_name.as_deref(), Some("observer-1"));
    assert_eq!(split.claude_args, toks(&["hi"]));
}

#[test]
fn split_tab_and_split_are_mutually_exclusive() {
    let err = split_launch_args(toks(&["--tab", "--split", "l"])).unwrap_err();
    assert!(err.to_string().contains("mutually exclusive"), "{err}");
    let err = split_launch_args(toks(&["--split", "d", "--tab"])).unwrap_err();
    assert!(err.to_string().contains("mutually exclusive"), "{err}");
    assert!(split_launch_args(toks(&["--panel", "--tab"])).is_ok());
}

#[test]
fn split_value_flag_without_value_and_bad_split_dir_are_usage_errors() {
    assert!(
        split_launch_args(toks(&["--run"]))
            .unwrap_err()
            .to_string()
            .contains("needs a value")
    );
    assert!(split_launch_args(toks(&["--split", "x"])).is_err());
    assert!(
        split_launch_args(toks(&["--json=1"]))
            .unwrap_err()
            .to_string()
            .contains("takes no value")
    );
}

#[test]
fn assemble_explicit_session_id_is_kept_where_the_caller_put_it() {
    let id = Uuid::new_v4();
    let spec = assemble(
        &[
            "--model",
            "opus",
            "--session-id",
            &id.to_string().to_uppercase(),
        ],
        None,
    );
    assert_eq!(spec.session, Some(id));
    assert!(!spec.preassigned);
    assert_eq!(spec.args[0], "--model");
    assert_eq!(
        spec.agent_name,
        format!("cc-{}", &id.simple().to_string()[..8])
    );
}

#[test]
fn assemble_invalid_explicit_session_id_is_usage_error() {
    let split = split_launch_args(toks(&["--session-id=nope"])).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let err = LaunchSpec::assemble(split, None, &opts(dir.path(), None)).unwrap_err();
    assert!(err.to_string().contains("not a uuid"), "{err}");
}

#[test]
fn assemble_remote_control_not_duplicated_when_caller_passed_it() {
    let spec = assemble(&["--remote-control"], None);
    assert_eq!(
        spec.args
            .iter()
            .filter(|a| a.as_str() == "--remote-control")
            .count(),
        1
    );
    let spec = assemble(&["--no-remote-control"], None);
    assert!(!spec.args.iter().any(|a| a == "--remote-control"));
}

#[test]
fn assemble_appends_the_channels_flag_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let split = split_launch_args(toks(&["--model", "opus"])).unwrap();
    let spec = LaunchSpec::assemble(split, None, &opts(dir.path(), None)).unwrap();
    assert!(
        spec.args
            .windows(2)
            .any(|w| w[0] == super::super::DEV_CHANNELS_FLAG && w[1] == "server:mike")
    );
}

#[test]
fn assemble_channels_flag_not_duplicated_when_caller_passed_it() {
    let dir = tempfile::tempdir().unwrap();
    let split = split_launch_args(toks(&[super::super::DEV_CHANNELS_FLAG, "server:mike"])).unwrap();
    let spec = LaunchSpec::assemble(split, None, &opts(dir.path(), None)).unwrap();
    assert_eq!(
        spec.args
            .iter()
            .filter(|a| a.as_str() == super::super::DEV_CHANNELS_FLAG)
            .count(),
        1
    );
}

#[test]
fn assemble_no_channels_omits_the_flag() {
    let spec = assemble(&["--no-channels"], None);
    assert!(
        !spec
            .args
            .iter()
            .any(|a| a == super::super::DEV_CHANNELS_FLAG)
    );
}

#[test]
fn assemble_substitutes_session_placeholder_in_args_and_brief() {
    let spec = assemble(
        &["--append-system-prompt", "you are {{session_id}}"],
        Some("brief for {{session_id}} here"),
    );
    let id = spec.session.unwrap().to_string();
    assert_eq!(spec.args[3], format!("you are {id}"));
    assert_eq!(
        spec.prompt.as_deref(),
        Some(format!("brief for {id} here").as_str())
    );
    assert_eq!(spec.args.last().unwrap(), &format!("brief for {id} here"));
}

#[test]
fn assemble_placeholder_without_session_id_is_usage_error() {
    let split = split_launch_args(toks(&["--continue", "see {{session_id}}"])).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let err = LaunchSpec::assemble(split, None, &opts(dir.path(), None)).unwrap_err();
    assert!(err.to_string().contains("{{session_id}}"), "{err}");
}

#[test]
fn assemble_prompt_file_with_bare_prompt_is_refused() {
    let split = split_launch_args(toks(&["--model", "opus", "hi"])).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let err =
        LaunchSpec::assemble(split, Some("from file".into()), &opts(dir.path(), None)).unwrap_err();
    assert!(err.to_string().contains("bare prompt"), "{err}");
}

#[test]
fn assemble_long_prompt_is_staged_to_file_named_by_session() {
    let dir = tempfile::tempdir().unwrap();
    let split = split_launch_args(toks(&[])).unwrap();
    let long = "z".repeat(801);
    let spec = LaunchSpec::assemble(split, Some(long.clone()), &opts(dir.path(), None)).unwrap();
    let brief = spec.brief.unwrap();
    assert_eq!(brief.mode, BriefMode::File);
    assert_eq!(
        brief.path.unwrap(),
        dir.path().join(format!("{}.md", spec.session.unwrap()))
    );
    assert_eq!(spec.prompt.as_deref(), Some(long.as_str()));
    assert!(
        spec.args
            .last()
            .unwrap()
            .starts_with("Your brief is saved at ")
    );
}

#[test]
fn assemble_env_comes_from_proxy_unless_opted_out() {
    let proxy = ProxyEnv {
        https_proxy: "http://127.0.0.1:9987".into(),
        no_proxy: "localhost,127.0.0.1".into(),
        ca_cert: "/r/.deploy/proxy-forward-ca.pem".into(),
    };
    let dir = tempfile::tempdir().unwrap();
    let with = LaunchSpec::assemble(
        split_launch_args(toks(&[])).unwrap(),
        None,
        &opts(dir.path(), Some(&proxy)),
    )
    .unwrap();
    assert_eq!(
        with.env.len(),
        4,
        "three proxy names and the session's own id"
    );
    assert_eq!(
        with.env.get("MIKE_SESSION_ID").map(String::as_str),
        with.session.map(|s| s.to_string()).as_deref()
    );
    let without = LaunchSpec::assemble(
        split_launch_args(toks(&["--no-proxy-env"])).unwrap(),
        None,
        &opts(dir.path(), Some(&proxy)),
    )
    .unwrap();
    assert_eq!(
        without.env.keys().collect::<Vec<_>>(),
        vec!["MIKE_SESSION_ID"]
    );
    assert_eq!(with.program, "claude");
}

#[test]
fn a_seat_learns_its_run_and_seat_from_its_environment() {
    let dir = tempfile::tempdir().unwrap();
    let spec = LaunchSpec::assemble(
        split_launch_args(toks(&["--run", "fleet-x-1", "--seat", "db-dev-T1"])).unwrap(),
        None,
        &opts(dir.path(), None),
    )
    .unwrap();
    assert_eq!(
        spec.env.get("MIKE_RUN_ID").map(String::as_str),
        Some("fleet-x-1")
    );
    assert_eq!(
        spec.env.get("MIKE_SEAT").map(String::as_str),
        Some("db-dev-T1")
    );
}

#[test]
fn a_seats_shell_returns_to_the_checkout_after_every_command() {
    let dir = tempfile::tempdir().unwrap();
    let seat = LaunchSpec::assemble(
        split_launch_args(toks(&["--run", "fleet-x-1", "--seat", "db-dev-T1"])).unwrap(),
        None,
        &opts(dir.path(), None),
    )
    .unwrap();
    assert_eq!(
        seat.env
            .get(super::SHELL_RETURNS_TO_THE_CHECKOUT)
            .map(String::as_str),
        Some("1"),
        "a seat whose shell stays in a subfolder makes Claude Code create .claude/agent-memory there"
    );
    let alone = LaunchSpec::assemble(
        split_launch_args(toks(&[])).unwrap(),
        None,
        &opts(dir.path(), None),
    )
    .unwrap();
    assert!(!alone.env.contains_key(super::SHELL_RETURNS_TO_THE_CHECKOUT));
}

#[test]
fn bare_prompt_detection_is_best_effort_as_the_contract_says() {
    assert!(!bare_prompt_present(&toks(&["--agent", "w"])));
    assert!(bare_prompt_present(&toks(&["--model", "opus", "hi"])));
    assert!(bare_prompt_present(&toks(&["hi"])));
    assert!(bare_prompt_present(&toks(&["--model=opus", "hi"])));
    assert!(!bare_prompt_present(&toks(&[
        "--verbose",
        "--model",
        "opus"
    ])));
}

#[test]
fn slug_collapses_whitespace_and_keeps_32_chars() {
    assert_eq!(slug("fix   the\n bug"), "fix the bug");
    let long = "a".repeat(40);
    assert_eq!(slug(&long), "a".repeat(32));
    assert_eq!(title(Some("opus"), Some("  ")), "cc·opus");
    assert_eq!(title(None, None), "cc");
}

#[test]
fn a_template_id_is_read_as_a_number_and_anything_else_is_refused() {
    let split =
        split_launch_args(toks(&["/repo", "--template-id", "42", "--model", "sonnet"])).unwrap();
    assert_eq!(split.template_id, Some(42));
    assert_eq!(split.claude_args, toks(&["/repo", "--model", "sonnet"]));
    let refused = split_launch_args(toks(&["/repo", "--template-id", "rust-developer"]));
    assert!(
        matches!(refused, Err(LaunchError::Usage(message)) if message.contains("not a template id"))
    );
}
