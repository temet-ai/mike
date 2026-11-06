use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{Value, json};

use super::super::{McpArgs, McpCommand, registration, unregister};
use super::plan;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: McpArgs,
}

#[test]
fn unregister_cli_selects_one_client_without_force_or_replace() -> Result<()> {
    let cli = Cli::try_parse_from([
        "mcp",
        "unregister",
        "--binary",
        "/tmp/bin with spaces/mike-mcp",
        "--claude-config",
        "/tmp/selected client.json",
        "--json",
    ])?;
    let McpCommand::Unregister(args) = cli.args.command else {
        anyhow::bail!("expected unregistration command");
    };
    assert!(args.json);
    assert_eq!(
        args.binary.as_deref(),
        Some(Path::new("/tmp/bin with spaces/mike-mcp"))
    );
    assert_eq!(
        args.claude_config.as_deref(),
        Some(Path::new("/tmp/selected client.json"))
    );
    for flag in ["--replace", "--force", "--all", "--scan"] {
        assert!(Cli::try_parse_from(["mcp", "unregister", flag]).is_err());
    }
    Ok(())
}

#[test]
fn unregister_owned_entry_preserves_other_servers_secrets_projects_and_trust() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let document = json!({
        "oauth": {"fake": "SECRET_SENTINEL"},
        "mcpServers": {"mike": plan.entry(), "other": {"command": "keep"}},
        "projects": {"/project": {"mcpServers": {"mike": plan.entry()}}},
        "permissions": {"allow": ["mcp__mike__*"], "deny": ["x"]},
        "allowedMcpServers": ["mike"],
        "hasTrustDialogAccepted": true
    });
    let original = serde_json::to_vec(&document)?;
    std::fs::write(&plan.client_config, &original)?;
    let other_client = directory.path().join("other-client.json");
    std::fs::write(&other_client, &original)?;
    let report = unregister(plan.clone())?;
    assert!(report.changed);
    assert_eq!(report.client_config, plan.client_config);
    let backup = report.backup.context("unregistration backup")?;
    assert_eq!(std::fs::read(&backup)?, original);
    assert_eq!(std::fs::read(other_client)?, original);
    let actual: Value = serde_json::from_slice(&std::fs::read(&plan.client_config)?)?;
    let mut expected = document;
    expected["mcpServers"]
        .as_object_mut()
        .context("server object")?
        .remove("mike");
    assert_eq!(actual, expected);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&plan.client_config, &backup] {
            assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
        }
    }
    let unchanged = std::fs::read(&plan.client_config)?;
    let repeated = unregister(plan.clone())?;
    assert!(!repeated.changed);
    assert!(repeated.backup.is_none());
    assert_eq!(std::fs::read(plan.client_config)?, unchanged);
    Ok(())
}

#[test]
fn unregister_changed_or_foreign_entries_are_preserved_byte_for_byte() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let mut entries = Vec::new();
    for (field, value) in [
        ("command", json!("/another-installation/bin/mike-mcp")),
        ("args", json!(["--core", "--other-option"])),
        (
            "env",
            json!({
                "MIKE_INSPECT_URL": "http://127.0.0.1:14590",
                "MIKE_REGISTRATION_INSTANCE_ID": plan.instance_id
            }),
        ),
        ("disabled", json!(true)),
    ] {
        let mut changed = plan.entry();
        changed[field] = value;
        entries.push(changed);
    }
    let mut extra_environment = plan.entry();
    extra_environment["env"]["CUSTOM_SETTING"] = json!("keep");
    entries.extend([extra_environment, Value::Null, json!({}), json!("foreign")]);
    for entry in entries {
        let original = format!("{{ \"keep\": 42, \"mcpServers\": {{\"mike\":{entry}}} }}\n");
        std::fs::write(&plan.client_config, &original)?;
        let error = unregister(plan.clone())
            .err()
            .context("ownership mismatch must be refused")?;
        assert!(error.to_string().contains("changed or foreign"));
        assert_eq!(std::fs::read_to_string(&plan.client_config)?, original);
        assert_no_backups(directory.path())?;
    }
    Ok(())
}

#[test]
fn unregister_shared_binary_and_endpoint_require_matching_selected_config_identity() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let binary = directory.path().join("mike-mcp");
    std::fs::write(&binary, "not an executable image")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
    }
    let client = directory.path().join("selected-client.json");
    let mut config = crate::config::ProxyConfig::default();
    config.inspect_listen_addr = "127.0.0.1:14589".into();
    config.config_path = directory.path().join("installation-a.toml");
    let plan_a = super::super::plan(&config, Some(&binary), Some(&client))?;
    config.config_path = directory.path().join("installation-b.toml");
    let plan_b = super::super::plan(&config, Some(&binary), Some(&client))?;
    assert_eq!(plan_a.binary, plan_b.binary);
    assert_eq!(plan_a.inspect_url, plan_b.inspect_url);
    assert_ne!(plan_a.instance_id, plan_b.instance_id);
    assert_ne!(plan_a.entry(), plan_b.entry());
    assert!(registration::prepare(&plan_a, false)?.commit()?.0);
    let original = std::fs::read(&client)?;
    assert!(unregister(plan_b.clone()).is_err());
    assert!(registration::prepare(&plan_b, false).is_err());
    assert_eq!(std::fs::read(&client)?, original);
    assert_no_backups(directory.path())?;
    assert!(unregister(plan_a)?.changed);
    Ok(())
}

#[test]
fn unregister_unmarked_legacy_registration_refuses_implicit_ownership() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let legacy = json!({
        "command": plan.binary,
        "args": ["--core"],
        "env": {"MIKE_INSPECT_URL": plan.inspect_url}
    });
    assert_ne!(legacy, plan.entry());
    let original = format!("{{ \"keep\": 42, \"mcpServers\": {{\"mike\":{legacy}}} }}\n");
    std::fs::write(&plan.client_config, &original)?;
    let error = unregister(plan.clone())
        .err()
        .context("unmarked legacy ownership must be refused")?;
    assert!(error.to_string().contains("without an ownership marker"));
    assert!(registration::prepare(&plan, false).is_err());
    assert_eq!(std::fs::read_to_string(&plan.client_config)?, original);
    assert_no_backups(directory.path())?;
    let (changed, backup) = registration::prepare(&plan, true)?.commit()?;
    assert!(changed);
    assert_eq!(
        std::fs::read_to_string(backup.context("legacy backup")?)?,
        original
    );
    let adopted: Value = serde_json::from_slice(&std::fs::read(&plan.client_config)?)?;
    assert_eq!(adopted["mcpServers"]["mike"], plan.entry());
    assert_eq!(adopted["keep"], 42);
    assert!(unregister(plan)?.changed);
    Ok(())
}

#[test]
fn unregister_absent_registration_keeps_original_bytes_and_does_not_create_config() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let report = unregister(plan.clone())?;
    assert!(!report.changed);
    assert!(report.backup.is_none());
    assert!(!plan.client_config.exists());
    for original in [
        "{ \"keep\": 42 }\n",
        "{ \"mcpServers\": {} }\n",
        "{ \"mcpServers\": {\"other\":{\"command\":\"keep\"}} }\n",
    ] {
        std::fs::write(&plan.client_config, original)?;
        let report = unregister(plan.clone())?;
        assert!(!report.changed);
        assert!(report.backup.is_none());
        assert_eq!(std::fs::read_to_string(&plan.client_config)?, original);
    }
    assert_no_backups(directory.path())
}

#[test]
fn unregister_malformed_or_nonobject_config_is_preserved() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    for original in [
        "{broken",
        "null",
        "[]",
        "{\"mcpServers\":null}",
        "{\"mcpServers\":[]}",
    ] {
        std::fs::write(&plan.client_config, original)?;
        assert!(unregister(plan.clone()).is_err());
        assert_eq!(std::fs::read_to_string(&plan.client_config)?, original);
    }
    assert_no_backups(directory.path())
}

#[test]
fn unregister_and_register_share_lock_and_preserve_concurrent_writer() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    std::fs::write(
        &plan.client_config,
        serde_json::to_vec(&json!({"mcpServers": {"mike": plan.entry()}}))?,
    )?;
    let prepared = registration::prepare_removal(&plan)?;
    assert!(registration::prepare_removal(&plan).is_err());
    assert!(registration::prepare(&plan, true).is_err());
    let concurrent = "{\"saved_by_claude\":true}";
    std::fs::write(&plan.client_config, concurrent)?;
    assert!(prepared.commit().is_err());
    assert_eq!(std::fs::read_to_string(&plan.client_config)?, concurrent);
    assert_no_backups(directory.path())?;
    assert!(!unregister(plan)?.changed);
    Ok(())
}

#[test]
fn unregister_missing_config_detects_new_file_without_removing_it() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let prepared = registration::prepare_removal(&plan)?;
    let concurrent = json!({"mcpServers": {"mike": plan.entry()}}).to_string();
    std::fs::write(&plan.client_config, &concurrent)?;
    assert!(prepared.commit().is_err());
    assert_eq!(std::fs::read_to_string(&plan.client_config)?, concurrent);
    assert_no_backups(directory.path())
}

#[cfg(unix)]
#[test]
fn unregister_symlinks_hardlinks_and_unsafe_lock_are_refused() -> Result<()> {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir()?;
    let plan = plan(directory.path());
    let other = directory.path().join("other.json");
    let original = json!({"mcpServers": {"mike": plan.entry()}}).to_string();
    std::fs::write(&other, &original)?;
    symlink(&other, &plan.client_config)?;
    assert!(unregister(plan.clone()).is_err());
    assert_eq!(std::fs::read_to_string(&other)?, original);
    std::fs::remove_file(&plan.client_config)?;
    std::fs::hard_link(&other, &plan.client_config)?;
    assert!(unregister(plan.clone()).is_err());
    assert_eq!(std::fs::read_to_string(&other)?, original);
    std::fs::remove_file(&plan.client_config)?;
    std::fs::write(&plan.client_config, &original)?;
    let lock = directory
        .path()
        .join(".synthetic-claude.json.mike-registration.lock");
    std::fs::remove_file(&lock)?;
    symlink(&other, &lock)?;
    assert!(unregister(plan.clone()).is_err());
    assert_eq!(std::fs::read_to_string(&plan.client_config)?, original);
    assert_eq!(std::fs::read_to_string(&other)?, original);
    assert_no_backups(directory.path())
}

#[test]
fn unregister_missing_parent_or_directory_destination_is_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut plan = plan(directory.path());
    std::fs::create_dir(&plan.client_config)?;
    assert!(unregister(plan.clone()).is_err());
    plan.client_config = directory.path().join("missing/config.json");
    assert!(unregister(plan).is_err());
    assert!(!directory.path().join("missing").exists());
    assert_no_backups(directory.path())
}

#[cfg(unix)]
#[test]
fn unregister_selected_plan_does_not_execute_companion_or_contact_inspect() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir()?;
    let binary = directory.path().join("mike-mcp");
    std::fs::write(&binary, "not an executable image")?;
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let mut config = crate::config::ProxyConfig::default();
    config.config_path = directory.path().join("selected-installation.toml");
    config.inspect_listen_addr = listener.local_addr()?.to_string();
    let client = directory.path().join("selected-client.json");
    let plan = super::super::plan(&config, Some(&binary), Some(&client))?;
    std::fs::write(
        &client,
        serde_json::to_vec(&json!({"mcpServers": {"mike": plan.entry()}}))?,
    )?;
    assert!(unregister(plan)?.changed);
    let error = listener
        .accept()
        .err()
        .context("unregister must not contact the daemon")?;
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(std::fs::read_to_string(binary)?, "not an executable image");
    Ok(())
}

fn assert_no_backups(directory: &Path) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        assert!(
            !entry?
                .file_name()
                .to_string_lossy()
                .contains(".mike-backup-")
        );
    }
    Ok(())
}
