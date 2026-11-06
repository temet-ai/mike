use std::path::PathBuf;

use serde_json::Value;

use super::super::apply::{
    ShellRequest, add_function, add_mods, choose_route, choose_trust, connect_mcp, disconnect_mcp,
    remove_functions, remove_mods,
};
use super::super::route::{self, Route};
use super::super::settings::{self, AUTO_TRUST_KEY, FUNCTION_KEY};
use super::super::shell::{BEGIN, Shell};
use super::super::status::gather;
use super::Machine;

const URL: &str = "http://127.0.0.1:9989";

fn request(shells: Vec<Shell>, route: Route) -> ShellRequest {
    ShellRequest {
        name: "cc".to_string(),
        shells,
        route,
        proxy_addr: "127.0.0.1:9988".to_string(),
    }
}

fn all_passed(findings: &[crate::finding::Finding]) -> bool {
    crate::finding::all_passed(findings)
}

#[test]
fn choosing_a_route_stores_it_in_the_config_file_and_reads_it_back() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let findings = choose_route(&machine.locations, Route::Plain)?;
    assert!(all_passed(&findings), "{findings:?}");
    assert_eq!(
        route::stored(&machine.locations.config_file)?,
        Some(Route::Plain)
    );
    choose_route(&machine.locations, Route::Sidecar)?;
    assert_eq!(
        route::stored(&machine.locations.config_file)?,
        Some(Route::Sidecar)
    );
    Ok(())
}

#[test]
fn trust_defaults_to_nothing_until_chosen_and_stores_both_answers() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let config = &machine.locations.config_file;
    assert_eq!(settings::read_bool(config, AUTO_TRUST_KEY)?, None);
    choose_trust(&machine.locations, false)?;
    assert_eq!(settings::read_bool(config, AUTO_TRUST_KEY)?, Some(false));
    choose_trust(&machine.locations, true)?;
    assert_eq!(settings::read_bool(config, AUTO_TRUST_KEY)?, Some(true));
    Ok(())
}

#[test]
fn adding_the_function_writes_each_chosen_rc_file_and_remembers_the_name() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    machine.add_executable("mike", "#!/bin/sh\n")?;
    let findings = add_function(
        &machine.locations,
        &request(vec![Shell::Zsh, Shell::Fish], Route::Sidecar),
        None,
    )?;
    assert_eq!(findings.len(), 2);
    assert!(all_passed(&findings), "{findings:?}");
    let zshrc = std::fs::read_to_string(machine.home().join(".zshrc"))?;
    assert!(
        zshrc.contains(BEGIN) && zshrc.contains("command mike claude"),
        "{zshrc}"
    );
    let fish = std::fs::read_to_string(machine.home().join(".config/fish/config.fish"))?;
    assert!(fish.contains("function cc"), "{fish}");
    assert!(!machine.home().join(".bashrc").exists());
    assert_eq!(
        settings::read(&machine.locations.config_file, FUNCTION_KEY)?.as_deref(),
        Some("cc")
    );
    Ok(())
}

#[test]
fn adding_the_function_again_with_another_route_replaces_the_block_in_place() -> anyhow::Result<()>
{
    let machine = Machine::new()?;
    let rc = machine.home().join(".zshrc");
    std::fs::write(&rc, "export A=1\n")?;
    add_function(
        &machine.locations,
        &request(vec![Shell::Zsh], Route::Sidecar),
        None,
    )?;
    let again = add_function(
        &machine.locations,
        &request(vec![Shell::Zsh], Route::Plain),
        None,
    )?;
    assert!(all_passed(&again), "{again:?}");
    let text = std::fs::read_to_string(&rc)?;
    assert_eq!(text.matches(BEGIN).count(), 1, "{text}");
    assert!(text.starts_with("export A=1\n"), "{text}");
    assert!(
        text.contains("ANTHROPIC_BASE_URL=http://127.0.0.1:9988"),
        "{text}"
    );
    let unchanged = add_function(
        &machine.locations,
        &request(vec![Shell::Zsh], Route::Plain),
        None,
    )?;
    assert!(unchanged[0].text.contains("already has"), "{unchanged:?}");
    Ok(())
}

#[test]
fn a_name_that_is_a_command_is_refused_before_any_file_is_written() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    machine.add_executable("cc", "#!/bin/sh\n")?;
    let result = add_function(
        &machine.locations,
        &request(vec![Shell::Zsh], Route::Sidecar),
        None,
    );
    assert!(result.is_err());
    assert!(!machine.home().join(".zshrc").exists());
    Ok(())
}

#[test]
fn no_shell_at_all_is_an_error_that_names_the_flag() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let error = add_function(
        &machine.locations,
        &request(Vec::new(), Route::Sidecar),
        None,
    )
    .err()
    .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(error.to_string().contains("--shell"), "{error}");
    Ok(())
}

#[test]
fn removing_the_functions_leaves_the_rest_of_each_rc_file() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let rc = machine.home().join(".bashrc");
    std::fs::write(&rc, "alias ll='ls -l'\n")?;
    add_function(
        &machine.locations,
        &request(vec![Shell::Bash], Route::Sidecar),
        None,
    )?;
    let findings = remove_functions(&machine.locations)?;
    assert!(all_passed(&findings));
    assert_eq!(std::fs::read_to_string(&rc)?, "alias ll='ls -l'\n");
    Ok(())
}

#[test]
fn mods_are_added_recorded_and_removed_without_touching_foreign_entries() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let root = machine.dir.path().join("share");
    for name in ["mike", "mods", "wiretap"] {
        let dir = root.join(name).join(".claude-plugin");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("plugin.json"), "{}")?;
    }
    let settings_file = machine.locations.claude_settings();
    std::fs::create_dir_all(
        settings_file
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent"))?,
    )?;
    std::fs::write(
        &settings_file,
        r#"{"env":{"CLAUDE_CODE_PLUGIN_DIRS":"/foreign"}}"#,
    )?;
    let findings = add_mods(&machine.locations, &root, false)?;
    assert!(all_passed(&findings), "{findings:?}");
    let status = gather(&machine.locations);
    assert!(status.mods_in_place());
    assert_eq!(status.recorded_mods.len(), 2);
    assert!(!status.recorded_mods.iter().any(|d| d.ends_with("wiretap")));
    let removed = remove_mods(&machine.locations)?;
    assert!(all_passed(&removed));
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&settings_file)?)?;
    assert_eq!(doc["env"]["CLAUDE_CODE_PLUGIN_DIRS"], "/foreign");
    assert!(!gather(&machine.locations).mods_in_place());
    Ok(())
}

#[test]
fn a_mods_folder_with_no_mods_is_an_error() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let empty: PathBuf = machine.dir.path().join("empty");
    std::fs::create_dir_all(&empty)?;
    assert!(add_mods(&machine.locations, &empty, false).is_err());
    Ok(())
}

#[tokio::test]
async fn connecting_the_mcp_server_writes_an_entry_with_the_binary_found_on_path()
-> anyhow::Result<()> {
    let machine = Machine::new()?;
    let binary = machine.add_executable("mike-mcp", super::probe_tests::ANSWERING_SCRIPT)?;
    let findings = connect_mcp(&machine.locations, URL, false).await?;
    assert!(all_passed(&findings), "{findings:?}");
    assert!(
        findings[0].text.contains(&binary.display().to_string()),
        "{findings:?}"
    );
    let removed = disconnect_mcp(&machine.locations)?;
    assert!(all_passed(&removed));
    Ok(())
}

#[tokio::test]
async fn connecting_without_the_binary_names_how_to_get_it() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let result = connect_mcp(&machine.locations, URL, false).await;
    let error = result
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected an error"))?;
    assert!(error.to_string().contains("brew install"), "{error}");
    assert!(!machine.locations.claude_json().exists());
    Ok(())
}

#[test]
fn the_status_names_what_is_missing_on_a_fresh_machine() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let status = gather(&machine.locations);
    let text = status.render(&machine.locations);
    assert!(text.contains("mcp: not registered"), "{text}");
    assert!(text.contains("route: not chosen"), "{text}");
    assert!(text.contains("mods: not installed"), "{text}");
    assert!(text.contains("folder trust: not chosen"), "{text}");
    Ok(())
}
