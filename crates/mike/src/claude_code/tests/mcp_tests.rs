use serde_json::{Value, json};

use super::super::atomic::{Edited, Removal};
use super::super::mcp::{self, entry_for, is_ours};
use super::Machine;

const URL: &str = "http://127.0.0.1:9989";

#[test]
fn the_entry_carries_the_absolute_command_and_the_inspect_address() {
    let entry = entry_for(std::path::Path::new("/opt/homebrew/bin/mike-mcp"), URL);
    assert_eq!(
        entry,
        json!({
            "type": "stdio",
            "command": "/opt/homebrew/bin/mike-mcp",
            "args": [],
            "env": {"MIKE_INSPECT_URL": URL}
        })
    );
}

#[test]
fn registering_keeps_other_servers_and_other_keys() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let file = machine.locations.claude_json();
    std::fs::write(
        &file,
        r#"{"theme":"dark","mcpServers":{"other":{"command":"x","args":[]}}}"#,
    )?;
    let binary = machine.add_executable("mike-mcp", "#!/bin/sh\n")?;
    let edited = mcp::register(&machine.locations, &binary, URL)?;
    assert_eq!(edited, Edited::Written);
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
    assert_eq!(doc["theme"], "dark");
    assert_eq!(doc["mcpServers"]["other"]["command"], "x");
    assert_eq!(doc["mcpServers"]["mike"]["env"]["MIKE_INSPECT_URL"], URL);
    assert_eq!(
        doc["mcpServers"]["mike"]["command"],
        binary.display().to_string()
    );
    Ok(())
}

#[test]
fn registering_twice_changes_nothing_the_second_time() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let binary = machine.add_executable("mike-mcp", "#!/bin/sh\n")?;
    mcp::register(&machine.locations, &binary, URL)?;
    assert_eq!(
        mcp::register(&machine.locations, &binary, URL)?,
        Edited::Unchanged
    );
    Ok(())
}

#[test]
fn registering_into_a_file_where_mcp_servers_is_a_string_fails_and_keeps_the_file()
-> anyhow::Result<()> {
    let machine = Machine::new()?;
    let file = machine.locations.claude_json();
    std::fs::write(&file, r#"{"mcpServers":"oops"}"#)?;
    let binary = machine.add_executable("mike-mcp", "#!/bin/sh\n")?;
    assert!(mcp::register(&machine.locations, &binary, URL).is_err());
    assert_eq!(std::fs::read_to_string(&file)?, r#"{"mcpServers":"oops"}"#);
    Ok(())
}

#[test]
fn unregistering_removes_only_an_entry_that_starts_mike_mcp() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let file = machine.locations.claude_json();
    std::fs::write(
        &file,
        r#"{"mcpServers":{"mike":{"command":"/usr/bin/some-other-tool"},"keep":{"command":"y"}}}"#,
    )?;
    assert_eq!(mcp::unregister(&machine.locations)?, Removal::NotOurs);
    std::fs::write(
        &file,
        r#"{"mcpServers":{"mike":{"command":"/x/mike-mcp"},"keep":{"command":"y"}}}"#,
    )?;
    assert_eq!(mcp::unregister(&machine.locations)?, Removal::Removed);
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
    assert!(doc["mcpServers"].get("mike").is_none());
    assert_eq!(doc["mcpServers"]["keep"]["command"], "y");
    assert_eq!(mcp::unregister(&machine.locations)?, Removal::NotThere);
    Ok(())
}

#[test]
fn unregistering_with_no_file_creates_none() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    assert_eq!(mcp::unregister(&machine.locations)?, Removal::NotThere);
    assert!(!machine.locations.claude_json().exists());
    Ok(())
}

#[test]
fn inspect_reports_whether_the_registered_command_exists() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let missing = machine.home().join("gone").join("mike-mcp");
    mcp::register(&machine.locations, &missing, URL)?;
    let facts = mcp::inspect(&machine.locations)?;
    assert!(facts.user_scope());
    assert!(!facts.command_exists);
    assert_eq!(facts.inspect_url.as_deref(), Some(URL));
    let present = machine.add_executable("mike-mcp", "#!/bin/sh\n")?;
    mcp::register(&machine.locations, &present, URL)?;
    assert!(mcp::inspect(&machine.locations)?.command_exists);
    Ok(())
}

#[test]
fn inspect_sees_a_project_scope_registration() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    std::fs::write(
        machine.locations.claude_json(),
        r#"{"projects":{"/p":{"mcpServers":{"mike":{"command":"mike-mcp"}}}}}"#,
    )?;
    let facts = mcp::inspect(&machine.locations)?;
    assert!(!facts.user_scope());
    assert!(facts.in_a_project);

    Ok(())
}

#[test]
fn is_ours_checks_the_command_file_name() {
    assert!(is_ours(&json!({"command": "mike-mcp"})));
    assert!(is_ours(&json!({"command": "/a/b/mike-mcp"})));
    assert!(!is_ours(&json!({"command": "/a/b/mike-mcp-fork"})));
    assert!(!is_ours(&json!({"args": []})));
}

#[test]
fn locate_binary_prefers_the_one_beside_mike_then_path() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let beside_dir = machine.dir.path().join("prefix").join("bin");
    std::fs::create_dir_all(&beside_dir)?;
    let mike = beside_dir.join("mike");
    std::fs::write(&mike, "x")?;
    let beside = beside_dir.join("mike-mcp");
    std::fs::write(&beside, "x")?;
    let on_path = machine.add_executable("mike-mcp", "#!/bin/sh\n")?;
    let found = mcp::locate_binary(Some(&mike), machine.locations.path_var.as_deref());
    assert_eq!(found, Some(std::fs::canonicalize(&beside)?));
    let without_exe = mcp::locate_binary(None, machine.locations.path_var.as_deref());
    assert_eq!(without_exe, Some(std::fs::canonicalize(&on_path)?));
    Ok(())
}

#[test]
fn locate_binary_finds_nothing_when_it_is_nowhere() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    assert_eq!(
        mcp::locate_binary(None, machine.locations.path_var.as_deref()),
        None
    );
    Ok(())
}
