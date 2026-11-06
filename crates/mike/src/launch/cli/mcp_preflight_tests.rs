use std::path::Path;

use super::*;
use crate::config::LaunchConfig;
use crate::launch::split_launch_args;

fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text)?;
    Ok(())
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn seat<'a>(config: &'a Path, folder: &'a Path, claude_args: &'a [String]) -> Seat<'a> {
    Seat {
        config_path: config,
        folder,
        claude_args,
        server_bin: Some(Path::new("/opt/mike/bin/mike-mcp")),
        inspect_addr: Some("127.0.0.1:9989"),
    }
}

fn refused(result: Result<McpPlan, LaunchError>) -> anyhow::Result<String> {
    match result {
        Err(LaunchError::Refused(why)) => Ok(why),
        other => anyhow::bail!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn plan_user_scope_server_is_configured() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    write(&config, r#"{"mcpServers":{"mike":{"command":"mike-mcp"}}}"#)?;
    assert_eq!(plan(&seat(&config, home.path(), &[]))?, McpPlan::Configured);
    Ok(())
}

#[test]
fn plan_project_scope_or_mcp_json_in_an_ancestor_is_configured() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    let lab = home.path().join("lab");
    let folder = lab.join("crates");
    std::fs::create_dir_all(&folder)?;
    let scoped =
        serde_json::json!({"projects": {lab.to_string_lossy(): {"mcpServers": {"mike": {}}}}});
    write(&config, &scoped.to_string())?;
    assert_eq!(plan(&seat(&config, &folder, &[]))?, McpPlan::Configured);

    write(&config, "{}")?;
    write(&lab.join(".mcp.json"), r#"{"mcpServers":{"mike":{}}}"#)?;
    assert_eq!(plan(&seat(&config, &folder, &[]))?, McpPlan::Configured);
    Ok(())
}

#[test]
fn plan_missing_server_is_passed_with_mcp_config_naming_the_binary_and_proxy() -> anyhow::Result<()>
{
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    write(
        &config,
        r#"{"mcpServers":{},"projects":{"/srv/elsewhere":{"mcpServers":{"mike":{}}}}}"#,
    )?;
    let McpPlan::Inject(inline) = plan(&seat(&config, home.path(), &[]))? else {
        anyhow::bail!("expected the server to be passed explicitly");
    };
    let value: Value = serde_json::from_str(&inline)?;
    assert_eq!(
        value["mcpServers"]["mike"]["command"],
        "/opt/mike/bin/mike-mcp"
    );
    assert_eq!(
        value["mcpServers"]["mike"]["env"]["MIKE_INSPECT_URL"],
        "http://127.0.0.1:9989"
    );
    Ok(())
}

#[test]
fn plan_absent_config_file_counts_as_no_server() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    assert!(matches!(
        plan(&seat(&config, home.path(), &[]))?,
        McpPlan::Inject(_)
    ));
    Ok(())
}

#[test]
fn plan_missing_server_and_no_binary_refuses_with_the_remedy() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    write(&config, "{}")?;
    let mut lacking = seat(&config, home.path(), &[]);
    lacking.server_bin = None;
    let why = refused(plan(&lacking))?;
    assert!(
        why.contains(&home.path().to_string_lossy().into_owned()),
        "{why}"
    );
    assert!(why.contains("make install"), "{why}");
    assert!(why.contains("looked for mike-mcp at"), "{why}");
    assert!(why.contains("claude mcp add --scope user mike"), "{why}");
    Ok(())
}

#[test]
fn plan_server_disabled_for_the_folder_refuses() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    let disabled = serde_json::json!({
        "mcpServers": {"mike": {}},
        "projects": {home.path().to_string_lossy(): {"disabledMcpServers": ["mike"]}}
    });
    write(&config, &disabled.to_string())?;
    let why = refused(plan(&seat(&config, home.path(), &[])))?;
    assert!(why.contains("disabled"), "{why}");
    Ok(())
}

#[test]
fn plan_unreadable_config_passes_the_server_and_only_warns_without_a_binary() -> anyhow::Result<()>
{
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    write(&config, "{ not json")?;
    assert!(matches!(
        plan(&seat(&config, home.path(), &[]))?,
        McpPlan::Inject(_)
    ));
    let mut lacking = seat(&config, home.path(), &[]);
    lacking.server_bin = None;
    assert!(matches!(plan(&lacking)?, McpPlan::Unknown(_)));
    Ok(())
}

#[test]
fn plan_strict_mcp_config_needs_mike_among_the_passed_configs() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join(".claude.json");
    write(&config, r#"{"mcpServers":{"mike":{}}}"#)?;
    let strict = args(&["--strict-mcp-config", "--model", "opus"]);
    assert!(matches!(
        plan(&seat(&config, home.path(), &strict))?,
        McpPlan::Inject(_)
    ));

    let file = home.path().join("servers.json");
    write(&file, r#"{"mcpServers":{"mike":{}}}"#)?;
    let with_file = args(&[
        "--strict-mcp-config",
        "--mcp-config",
        &file.to_string_lossy(),
    ]);
    assert_eq!(
        plan(&seat(&config, home.path(), &with_file))?,
        McpPlan::Configured
    );
    let inline = args(&[
        r#"--mcp-config={"mcpServers":{"mike":{}}}"#,
        "--strict-mcp-config",
    ]);
    assert_eq!(
        plan(&seat(&config, home.path(), &inline))?,
        McpPlan::Configured
    );
    Ok(())
}

#[test]
fn apply_inserts_the_config_first_and_drops_channel_push() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let backend = super::super::tests::fake();
    let config = LaunchConfig::trusting();
    let mut runner = super::super::tests::runner(&backend, &config, home.path());
    write(&home.path().join(".claude.json"), "{}")?;
    runner.mcp_server = Some(Path::new("/opt/mike/bin/mike-mcp").to_path_buf());

    let mut split = split_launch_args(args(&["--model", "opus"]))?;
    apply(&runner, home.path(), &mut split)?;
    assert!(
        split.claude_args[0].starts_with("--mcp-config={"),
        "{:?}",
        split.claude_args
    );
    assert_eq!(&split.claude_args[1..], &args(&["--model", "opus"])[..]);
    assert!(!split.channels);

    let mut explicit = split_launch_args(args(&[DEV_CHANNELS_FLAG, "server:mike"]))?;
    apply(&runner, home.path(), &mut explicit)?;
    assert!(explicit.claude_args[0].starts_with("--mcp-config={"));
    assert!(explicit.channels);
    Ok(())
}
