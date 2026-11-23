use super::*;
use crate::launch::split_launch_args;
use crate::runtime::RuntimeKind;

fn options(dir: &std::path::Path) -> AssembleOptions<'_> {
    AssembleOptions {
        program: "claude".into(),
        proxy: None,
        brief_inline_max: 100,
        briefs_dir: dir,
        brief_notes: true,
        brief_by_mail: true,
    }
}

fn tokens(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn runtime_selection_preassigns_mike_identity_without_claude_options() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let split = split_launch_args(tokens(&[
        "--runtime",
        "codex",
        "-m",
        "gpt-fixture",
        "hello",
    ]))?;
    let spec = LaunchSpec::assemble(split, None, &options(dir.path()))?;
    assert_eq!(spec.program, "codex");
    assert_eq!(
        spec.args,
        tokens(&["--no-daemon", "-m", "gpt-fixture", "--", "hello"])
    );
    assert_eq!(spec.flags.runtime, RuntimeKind::Codex);
    assert!(!spec.flags.remote_control && !spec.flags.channels);
    assert_eq!(
        spec.env.get("MIKE_CODEX_SPAWN_SESSION_ID"),
        spec.session.map(|session| session.to_string()).as_ref()
    );
    assert!(!spec.env.contains_key("HTTPS_PROXY"));
    Ok(())
}

#[test]
fn codex_long_brief_uses_file_without_claude_read_permission_or_mail_injection()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let split = split_launch_args(tokens(&["--runtime=codex"]))?;
    let prompt = "x".repeat(101);
    let spec = LaunchSpec::assemble(split, Some(prompt.clone()), &options(dir.path()))?;
    let brief = spec.brief.ok_or_else(|| anyhow::anyhow!("missing brief"))?;
    assert_eq!(brief.mode, BriefMode::File);
    let path = brief
        .path
        .ok_or_else(|| anyhow::anyhow!("missing brief path"))?;
    assert_eq!(std::fs::read_to_string(path)?, prompt);
    assert_eq!(spec.args.len(), 3);
    assert_eq!(spec.args[1], "--");
    assert!(spec.args[2].starts_with("Read your complete brief"));
    Ok(())
}

#[test]
fn external_prompt_cannot_become_a_codex_subcommand_or_option() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    for prompt in ["resume", "fork", "exec", "--help", "-C/another-project"] {
        let split = split_launch_args(tokens(&["--runtime=codex"]))?;
        let spec = LaunchSpec::assemble(split, Some(prompt.into()), &options(dir.path()))?;
        assert_eq!(spec.args, tokens(&["--no-daemon", "--", prompt]));
    }
    Ok(())
}

#[test]
fn delimiter_preserves_runtime_named_provider_argument_and_invalid_runtime_fails()
-> anyhow::Result<()> {
    let split = split_launch_args(tokens(&["--runtime", "codex", "--", "--runtime", "claude"]))?;
    assert_eq!(split.runtime, RuntimeKind::Codex);
    assert_eq!(split.claude_args, tokens(&["--runtime", "claude"]));
    assert!(split_launch_args(tokens(&["--runtime", "unknown"])).is_err());
    assert!(split_launch_args(tokens(&["-m", "sonnet"])).is_err());
    Ok(())
}
