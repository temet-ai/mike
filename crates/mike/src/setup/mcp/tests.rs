use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::{RegistrationPlan, registration};

#[path = "path_tests.rs"]
mod paths;
#[cfg(unix)]
#[path = "probe_tests.rs"]
mod protocol;
#[path = "unregister_tests.rs"]
mod unregister;

fn plan(directory: &std::path::Path) -> RegistrationPlan {
    RegistrationPlan {
        binary: directory.join("bin with spaces/mike-mcp"),
        inspect_url: "http://127.0.0.1:14589".to_string(),
        instance_id: "test-instance".into(),
        client_config: directory.join("synthetic-claude.json"),
    }
}

#[test]
fn registration_preserves_unrelated_data_and_exact_private_backup() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let original = br#"{ "oauth": {"fake":"SECRET_SENTINEL"}, "mcpServers":{"other":{"command":"other"}}, "permissions":{"deny":["x"]} }"#;
    std::fs::write(&plan.client_config, original)?;
    let (changed, backup) = registration::prepare(&plan, false)?.commit()?;
    assert!(changed);
    let backup = backup.context("existing config is backed up")?;
    assert_eq!(std::fs::read(&backup)?, original);
    let actual: Value = serde_json::from_slice(&std::fs::read(&plan.client_config)?)?;
    assert_eq!(actual["mcpServers"]["mike"], plan.entry());
    assert_eq!(
        actual["mcpServers"]["mike"]["env"]["MIKE_REGISTRATION_INSTANCE_ID"],
        plan.instance_id
    );
    assert_eq!(actual["mcpServers"]["other"], json!({"command":"other"}));
    assert_eq!(actual["oauth"], json!({"fake":"SECRET_SENTINEL"}));
    assert_eq!(actual["permissions"], json!({"deny":["x"]}));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&plan.client_config, &backup] {
            assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
        }
    }
    Ok(())
}

#[test]
fn registration_identical_entry_does_not_reformat_or_backup() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let original = format!(
        "{{\"other\":42,\"mcpServers\":{{\"mike\":{}}}}}",
        plan.entry()
    );
    std::fs::write(&plan.client_config, &original)?;
    assert_eq!(
        registration::prepare(&plan, false)?.commit()?,
        (false, None)
    );
    assert_eq!(std::fs::read_to_string(plan.client_config)?, original);
    Ok(())
}

#[test]
fn registration_conflicting_entry_requires_explicit_replace() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let original =
        br#"{"mcpServers":{"mike":{"command":"other-instance"},"keep":{"command":"safe"}}}"#;
    std::fs::write(&plan.client_config, original)?;
    assert!(registration::prepare(&plan, false).is_err());
    assert_eq!(std::fs::read(&plan.client_config)?, original);
    let (changed, backup) = registration::prepare(&plan, true)?.commit()?;
    assert!(changed);
    assert_eq!(
        std::fs::read(backup.context("replacement backup")?)?,
        original
    );
    let actual: Value = serde_json::from_slice(&std::fs::read(plan.client_config)?)?;
    assert_eq!(actual["mcpServers"]["keep"], json!({"command":"safe"}));
    Ok(())
}

#[test]
fn registration_malformed_or_nonobject_config_is_unchanged() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    for content in [
        "{broken",
        "null",
        "[]",
        "{\"mcpServers\":null}",
        "{\"mcpServers\":[]}",
    ] {
        std::fs::write(&plan.client_config, content)?;
        assert!(registration::prepare(&plan, true).is_err());
        assert_eq!(std::fs::read_to_string(&plan.client_config)?, content);
    }
    Ok(())
}

#[test]
fn registration_missing_config_creates_private_file_without_backup() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    assert_eq!(registration::prepare(&plan, false)?.commit()?, (true, None));
    let actual: Value = serde_json::from_slice(&std::fs::read(&plan.client_config)?)?;
    assert_eq!(actual, json!({"mcpServers":{"mike":plan.entry()}}));
    Ok(())
}

#[test]
fn registration_concurrent_writer_is_preserved() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    std::fs::write(&plan.client_config, "{}")?;
    let prepared = registration::prepare(&plan, false)?;
    assert!(registration::prepare(&plan, false).is_err());
    let new_content = "{\"saved_by_claude\":true}";
    std::fs::write(&plan.client_config, new_content)?;
    assert!(prepared.commit().is_err());
    assert_eq!(std::fs::read_to_string(&plan.client_config)?, new_content);
    Ok(())
}

#[cfg(unix)]
#[test]
fn registration_symlinks_and_hardlinks_are_refused() -> Result<()> {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let other = directory.path().join("other.json");
    symlink(&other, &plan.client_config)?;
    assert!(registration::prepare(&plan, true).is_err());
    assert!(!other.exists());
    std::fs::write(&other, "{}")?;
    assert!(registration::prepare(&plan, true).is_err());
    std::fs::remove_file(&plan.client_config)?;
    std::fs::hard_link(&other, &plan.client_config)?;
    assert!(registration::prepare(&plan, true).is_err());
    assert_eq!(std::fs::read_to_string(other)?, "{}");
    Ok(())
}

#[test]
fn registration_missing_parent_or_directory_destination_is_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut plan = plan(directory.path());
    std::fs::create_dir(&plan.client_config)?;
    assert!(registration::prepare(&plan, false).is_err());
    plan.client_config = directory.path().join("missing/config.json");
    assert!(registration::prepare(&plan, false).is_err());
    Ok(())
}

#[test]
fn cli_probe_and_explicit_registration_have_distinct_arguments() -> Result<()> {
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: super::McpArgs,
    }
    let probe = Cli::try_parse_from(["mcp", "probe", "--binary", "/tmp/mcp", "--json"])?;
    assert!(matches!(probe.args.command, super::McpCommand::Probe(_)));
    let registration = Cli::try_parse_from([
        "mcp",
        "register",
        "--binary",
        "/tmp/mcp",
        "--claude-config",
        "/tmp/client.json",
        "--replace",
        "--json",
    ])?;
    let super::McpCommand::Register(args) = registration.args.command else {
        anyhow::bail!("expected registration command");
    };
    assert!(args.replace);
    assert!(args.probe.json);
    assert_eq!(
        args.claude_config.as_deref(),
        Some(std::path::Path::new("/tmp/client.json"))
    );
    assert!(Cli::try_parse_from(["mcp", "probe", "--replace"]).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn registration_symlinked_lock_is_refused_without_touching_target() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let other = directory.path().join("other");
    std::fs::write(&other, "private sentinel")?;
    std::os::unix::fs::symlink(
        &other,
        directory
            .path()
            .join(".synthetic-claude.json.mike-registration.lock"),
    )?;
    assert!(registration::prepare(&plan, false).is_err());
    assert_eq!(std::fs::read_to_string(&other)?, "private sentinel");
    assert!(!plan.client_config.exists());
    Ok(())
}
