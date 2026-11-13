use std::io::Read;

use uuid::Uuid;

use super::open::{self, OpenFailure};
use super::runner::Runner;
use crate::launch::{
    LaunchError, Placement, PromptSource, SplitDirection, SplitLaunchArgs, split_launch_args,
};
use crate::store::persistence::launch::TemplateRow;

const PREFIX_FLAGS: [&str; 6] = [
    "--model",
    "--agent",
    "--append-system-prompt-file",
    "--settings",
    "--add-dir",
    "--mcp-config",
];

fn bare(error: LaunchError) -> OpenFailure {
    OpenFailure {
        output: None,
        error,
    }
}

fn take_target(tokens: Vec<String>) -> Result<(String, Vec<String>), LaunchError> {
    let mut iter = tokens.into_iter();
    let target = iter.next().filter(|t| !t.starts_with('-')).ok_or_else(|| {
        LaunchError::Usage(
            "fork needs a template: mike spawn fork <discipline|session-uuid> [--folder <dir>] \
             [claude args...]"
                .into(),
        )
    })?;
    Ok((target, iter.collect()))
}

fn strip_fork_flags(tokens: Vec<String>) -> (Vec<String>, Option<String>, bool) {
    let mut folder = None;
    let mut allow_prefix_change = false;
    let mut kept = Vec::with_capacity(tokens.len());
    let mut iter = tokens.into_iter();
    let mut verbatim = false;
    while let Some(token) = iter.next() {
        if !verbatim && token == "--folder" {
            folder = iter.next();
            continue;
        }
        if !verbatim && token == "--allow-prefix-change" {
            allow_prefix_change = true;
            continue;
        }
        if token == "--" {
            verbatim = true;
        }
        kept.push(token);
    }
    (kept, folder, allow_prefix_change)
}

async fn resolve_template(runner: &Runner<'_>, target: &str) -> Result<TemplateRow, LaunchError> {
    let found = match target.parse::<Uuid>() {
        Ok(session) => runner.ledger.find_template_by_session(session).await,
        Err(_) => runner.ledger.find_template_by_name(target).await,
    };
    found
        .map_err(|e| LaunchError::Failed(format!("could not look up template `{target}`: {e}")))?
        .ok_or_else(|| {
            LaunchError::Refused(format!(
                "no live template named or resolved from `{target}`; `mike spawn templates` lists them"
            ))
        })
}

fn template_args(template: &TemplateRow) -> Vec<String> {
    serde_json::from_value(template.claude_args.clone()).unwrap_or_default()
}

fn read_flag(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
        if let Some(value) = arg.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return Some(value.to_string());
        }
    }
    None
}

fn conflicting_prefix_flag(
    caller: &[String],
    recorded: &[String],
) -> Option<(&'static str, Option<String>, String)> {
    PREFIX_FLAGS.iter().find_map(|&flag| {
        let given = read_flag(caller, flag)?;
        let recorded_value = read_flag(recorded, flag);
        (recorded_value.as_deref() != Some(given.as_str())).then_some((flag, recorded_value, given))
    })
}

fn explicit_session(args: &[String]) -> Option<Uuid> {
    read_flag(args, "--session-id").and_then(|v| v.parse().ok())
}

fn push_valued_flag(out: &mut Vec<String>, flag: &str, value: Option<&String>) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(value.clone());
    }
}

fn push_toggle_flag(out: &mut Vec<String>, flag: &str, present: bool) {
    if present {
        out.push(flag.to_string());
    }
}

fn push_placement_tokens(out: &mut Vec<String>, placement: Option<Placement>) {
    match placement {
        Some(Placement::Tab) => out.push("--tab".to_string()),
        Some(Placement::Split(dir)) => {
            out.push("--split".to_string());
            out.push(
                match dir {
                    SplitDirection::Left => "l",
                    SplitDirection::Right => "r",
                    SplitDirection::Up => "u",
                    SplitDirection::Down => "d",
                }
                .to_string(),
            );
        }
        Some(Placement::Panel) => out.push("--panel".to_string()),
        Some(Placement::Window) | None => {}
    }
}

fn push_prompt_file_tokens(out: &mut Vec<String>, prompt_file: Option<&PromptSource>) {
    let value = match prompt_file {
        Some(PromptSource::File(path)) => path.display().to_string(),
        Some(PromptSource::Stdin) => "-".to_string(),
        None => return,
    };
    out.push("--prompt-file".to_string());
    out.push(value);
}

fn push_identity_tokens(out: &mut Vec<String>, split: &SplitLaunchArgs) {
    push_valued_flag(out, "--role", split.role.as_ref());
    push_valued_flag(out, "--instructions", split.instructions.as_ref());
    push_valued_flag(out, "--goal", split.goal.as_ref());
    push_valued_flag(out, "--session-name", split.session_name.as_ref());
}

fn spawn_flag_tokens(split: &SplitLaunchArgs) -> Vec<String> {
    let mut out = Vec::new();
    push_placement_tokens(&mut out, split.placement);
    push_valued_flag(
        &mut out,
        "--anchor-terminal",
        split.anchor_terminal.as_ref(),
    );
    push_toggle_flag(&mut out, "--json", split.json);
    push_toggle_flag(&mut out, "--worktree-ok", split.worktree_ok);
    push_toggle_flag(&mut out, "--no-carry-config", !split.carry_config);
    push_prompt_file_tokens(&mut out, split.prompt_file.as_ref());
    push_valued_flag(&mut out, "--run", split.run.as_ref());
    push_valued_flag(&mut out, "--seat", split.seat.as_ref());
    push_identity_tokens(&mut out, split);
    push_toggle_flag(&mut out, "--trust", split.trust);
    push_toggle_flag(&mut out, "--no-proxy-env", !split.proxy_env);
    push_toggle_flag(&mut out, "--no-remote-control", !split.remote_control);
    push_toggle_flag(&mut out, "--no-channels", !split.channels);
    out
}

pub async fn execute(
    runner: &Runner<'_>,
    tokens: Vec<String>,
    stdin: &mut (dyn Read + Send),
) -> open::OpenResult {
    let (target, rest) = take_target(tokens).map_err(bare)?;
    let (rest, folder_override, allow_prefix_change) = strip_fork_flags(rest);
    let template = resolve_template(runner, &target).await.map_err(bare)?;
    let recorded_args = template_args(&template);

    let split = split_launch_args(rest).map_err(bare)?;
    if !allow_prefix_change
        && let Some((flag, recorded, given)) =
            conflicting_prefix_flag(&split.claude_args, &recorded_args)
    {
        return Err(bare(LaunchError::Refused(format!(
            "{flag} {given} conflicts with the template's recorded {flag} {}; pass \
             --allow-prefix-change to launch anyway (this costs a full rewrite of the template \
             context on the first request)",
            recorded.as_deref().unwrap_or("(none)")
        ))));
    }

    let folder = folder_override.unwrap_or_else(|| template.folder.clone());
    let mut prefix = vec![
        "--resume".to_string(),
        template.session_uuid.to_string(),
        "--fork-session".to_string(),
    ];
    if explicit_session(&split.claude_args).is_none() {
        prefix.push("--session-id".to_string());
        prefix.push(Uuid::new_v4().to_string());
    }

    let mut synthetic = vec![folder];
    synthetic.extend(spawn_flag_tokens(&split));
    synthetic.push("--template-id".to_string());
    synthetic.push(template.id.to_string());
    synthetic.push("--".to_string());
    synthetic.extend(prefix);
    synthetic.extend(recorded_args);
    synthetic.extend(split.claude_args);

    open::execute(runner, synthetic, stdin).await
}

#[cfg(any(test, feature = "test-seams"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use super::*;

    #[test]
    fn take_target_reads_the_leading_token() -> anyhow::Result<()> {
        let (target, rest) = take_target(vec!["verifier".into(), "--tab".into()])?;
        assert_eq!(target, "verifier");
        assert_eq!(rest, vec!["--tab"]);
        Ok(())
    }

    #[test]
    fn take_target_missing_or_flag_first_is_usage_error() {
        assert!(take_target(vec![]).is_err());
        assert!(take_target(vec!["--tab".into()]).is_err());
    }

    #[test]
    fn strip_fork_flags_extracts_folder_and_allow_prefix_change() {
        let (kept, folder, allow) = strip_fork_flags(vec![
            "--tab".into(),
            "--folder".into(),
            "/other".into(),
            "--allow-prefix-change".into(),
            "--model".into(),
            "opus".into(),
        ]);
        assert_eq!(kept, vec!["--tab", "--model", "opus"]);
        assert_eq!(folder.as_deref(), Some("/other"));
        assert!(allow);
    }

    #[test]
    fn strip_fork_flags_after_double_dash_left_alone() {
        let (kept, folder, allow) =
            strip_fork_flags(vec!["--".into(), "--folder".into(), "literal".into()]);
        assert_eq!(kept, vec!["--", "--folder", "literal"]);
        assert_eq!(folder, None);
        assert!(!allow);
    }

    #[test]
    fn conflicting_prefix_flag_only_fires_on_a_real_difference() {
        let recorded = vec!["--model".to_string(), "opus".to_string()];
        assert_eq!(
            conflicting_prefix_flag(&["--model".to_string(), "sonnet".to_string()], &recorded),
            Some(("--model", Some("opus".to_string()), "sonnet".to_string()))
        );
        assert_eq!(
            conflicting_prefix_flag(&["--model".to_string(), "opus".to_string()], &recorded),
            None,
            "same value is not a conflict"
        );
        assert_eq!(
            conflicting_prefix_flag(&["--verbose".to_string()], &recorded),
            None,
            "an untouched flag is not a conflict"
        );
        assert_eq!(
            conflicting_prefix_flag(&["--agent".to_string(), "reviewer".to_string()], &[]),
            Some(("--agent", None, "reviewer".to_string())),
            "adding a prefix flag the template never had is still a conflict"
        );
    }

    #[test]
    fn explicit_session_reads_when_present() {
        let id = Uuid::new_v4();
        assert_eq!(
            explicit_session(&["--session-id".to_string(), id.to_string()]),
            Some(id)
        );
        assert_eq!(explicit_session(&["--resume".to_string()]), None);
    }

    #[test]
    fn spawn_flag_tokens_round_trips_every_field() {
        let mut split = split_launch_args(vec![
            "--tab".to_string(),
            "--json".to_string(),
            "--worktree-ok".to_string(),
            "--run".to_string(),
            "run-1".to_string(),
            "--seat".to_string(),
            "verifier".to_string(),
            "--role".to_string(),
            "observer".to_string(),
            "--instructions".to_string(),
            "watch the fleet".to_string(),
            "--goal".to_string(),
            "ship phase 1".to_string(),
            "--session-name".to_string(),
            "observer-1".to_string(),
            "--trust".to_string(),
        ])
        .unwrap();
        split.claude_args.clear();
        let tokens = spawn_flag_tokens(&split);
        let round_tripped = split_launch_args(tokens).unwrap();
        assert_eq!(round_tripped.placement, split.placement);
        assert_eq!(round_tripped.json, split.json);
        assert_eq!(round_tripped.worktree_ok, split.worktree_ok);
        assert_eq!(round_tripped.run, split.run);
        assert_eq!(round_tripped.seat, split.seat);
        assert_eq!(round_tripped.role, split.role);
        assert_eq!(round_tripped.instructions, split.instructions);
        assert_eq!(round_tripped.goal, split.goal);
        assert_eq!(round_tripped.session_name, split.session_name);
        assert_eq!(round_tripped.trust, split.trust);
        assert!(round_tripped.claude_args.is_empty());
    }

    #[test]
    fn synthetic_stream_shields_recorded_args_from_a_second_legacy_rejection_pass() {
        let split = split_launch_args(vec!["--json".to_string()]).unwrap();
        let recorded_args: Vec<String> = vec!["--print".into(), "-p".into(), "hi".into()];
        let prefix = vec![
            "--resume".to_string(),
            Uuid::nil().to_string(),
            "--fork-session".to_string(),
        ];

        let mut synthetic = vec!["/repo".to_string()];
        synthetic.extend(spawn_flag_tokens(&split));
        synthetic.push("--".to_string());
        synthetic.extend(prefix);
        synthetic.extend(recorded_args);
        synthetic.extend(split.claude_args.clone());

        let folder = synthetic.remove(0);
        assert_eq!(folder, "/repo");
        let resolved = split_launch_args(synthetic).unwrap();
        assert!(resolved.json);
        assert_eq!(
            resolved.claude_args,
            vec![
                "--resume",
                &Uuid::nil().to_string(),
                "--fork-session",
                "--print",
                "-p",
                "hi"
            ]
        );
    }
}
