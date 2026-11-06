use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::super::atomic::Removal;
use super::super::mods::{ENV_KEY, find_source, install, list, merged, uninstall};
use super::Machine;

fn make_mod(root: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join(".claude-plugin"))?;
    std::fs::write(dir.join(".claude-plugin").join("plugin.json"), "{}")?;
    Ok(dir)
}

fn strings(paths: &[&PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

#[test]
fn list_returns_every_mod_but_wiretap_unless_asked() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    make_mod(dir.path(), "mike")?;
    make_mod(dir.path(), "wiretap")?;
    make_mod(dir.path(), "mods")?;
    std::fs::create_dir_all(dir.path().join("not-a-mod"))?;
    let names = |with: bool| -> Vec<String> {
        list(dir.path(), with)
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
            .collect()
    };
    assert_eq!(names(false), vec!["mike", "mods"]);
    assert_eq!(names(true), vec!["mike", "mods", "wiretap"]);
    Ok(())
}

#[test]
fn a_package_install_finds_its_share_folder_beside_the_bin_folder() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let prefix = std::fs::canonicalize(dir.path())?.join("prefix");
    let share = prefix.join("share").join("mike").join("mods");
    make_mod(&share, "mike")?;
    std::fs::create_dir_all(prefix.join("bin"))?;
    let exe = prefix.join("bin").join("mike");
    std::fs::write(&exe, "x")?;
    assert_eq!(find_source(Some(&exe), None), Some(share));
    Ok(())
}

#[test]
fn an_archive_generation_finds_mods_from_the_resolved_binary() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let generation = std::fs::canonicalize(dir.path())?.join("generation");
    let share = generation.join("share/mike/mods");
    make_mod(&share, "mike")?;
    let exe = generation.join("mike");
    std::fs::write(&exe, "x")?;
    assert_eq!(find_source(Some(&exe), None), Some(share));
    Ok(())
}

#[test]
fn a_checkout_is_found_from_the_binary_or_the_working_folder() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let repo = std::fs::canonicalize(dir.path())?.join("repo");
    make_mod(&repo.join("mods"), "mike")?;
    let target = repo.join("target").join("debug");
    std::fs::create_dir_all(&target)?;
    let exe = target.join("mike");
    std::fs::write(&exe, "x")?;
    assert_eq!(find_source(Some(&exe), None), Some(repo.join("mods")));
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere)?;
    assert_eq!(
        find_source(None, Some(&repo.join("crates"))),
        Some(repo.join("mods"))
    );
    assert_eq!(find_source(None, Some(&elsewhere)), None);
    Ok(())
}

#[test]
fn merging_puts_ours_first_and_keeps_foreign_entries() {
    let root = Path::new("/share/mods");
    let ours = vec![
        "/share/mods/mike".to_string(),
        "/share/mods/mods".to_string(),
    ];
    let result = merged("/other/plugin:/elsewhere/x", &ours, &[], root);
    assert_eq!(
        result.value,
        "/share/mods/mike:/share/mods/mods:/other/plugin:/elsewhere/x"
    );
    assert_eq!(result.added, ours);
}

#[test]
fn merging_replaces_what_mike_wrote_before_and_anything_under_the_root() {
    let root = Path::new("/share/mods");
    let previous = vec!["/old/place/mike".to_string()];
    let ours = vec!["/share/mods/mike".to_string()];
    let existing = "/old/place/mike:/share/mods/gone:/share/mods/mike:/keep";
    let result = merged(existing, &ours, &previous, root);
    assert_eq!(result.value, "/share/mods/mike:/keep");
}

#[test]
fn merging_does_not_add_a_mod_the_user_already_links_to_the_same_folder() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = std::fs::canonicalize(dir.path())?.join("repo").join("mods");
    let mine = make_mod(&root, "mike")?;
    let links = std::fs::canonicalize(dir.path())?.join("links");
    std::fs::create_dir_all(&links)?;
    let link = links.join("mike");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&mine, &link)?;
    let result = merged(&link.display().to_string(), &strings(&[&mine]), &[], &root);
    assert_eq!(result.value, link.display().to_string());
    assert!(result.added.is_empty());
    Ok(())
}

#[test]
fn install_writes_the_env_value_and_keeps_every_other_settings_key() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let settings = machine.locations.claude_settings();
    std::fs::create_dir_all(
        settings
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent"))?,
    )?;
    std::fs::write(
        &settings,
        r#"{"model":"opus","env":{"FOO":"bar","CLAUDE_CODE_PLUGIN_DIRS":"/foreign/one"},"hooks":{"Stop":[]}}"#,
    )?;
    let root = machine.dir.path().join("share");
    let a = make_mod(&root, "mike")?;
    let b = make_mod(&root, "mods")?;
    let (_, added) = install(&machine.locations, &root, &[a.clone(), b.clone()], &[])?;
    assert_eq!(added, strings(&[&a, &b]));
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&settings)?)?;
    assert_eq!(doc["model"], "opus");
    assert_eq!(doc["env"]["FOO"], "bar");
    assert_eq!(doc["hooks"], json!({"Stop": []}));
    assert_eq!(
        doc["env"][ENV_KEY],
        format!("{}:{}:/foreign/one", a.display(), b.display())
    );
    Ok(())
}

#[test]
fn uninstall_removes_only_what_mike_wrote() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let settings = machine.locations.claude_settings();
    std::fs::create_dir_all(
        settings
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent"))?,
    )?;
    std::fs::write(
        &settings,
        r#"{"env":{"CLAUDE_CODE_PLUGIN_DIRS":"/m/mike:/m/mods:/foreign"}}"#,
    )?;
    let written = vec!["/m/mike".to_string(), "/m/mods".to_string()];
    assert_eq!(uninstall(&machine.locations, &written)?, Removal::Removed);
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&settings)?)?;
    assert_eq!(doc["env"][ENV_KEY], "/foreign");
    assert_eq!(uninstall(&machine.locations, &written)?, Removal::NotThere);
    std::fs::write(
        &settings,
        r#"{"env":{"CLAUDE_CODE_PLUGIN_DIRS":"/m/mike"}}"#,
    )?;
    assert_eq!(uninstall(&machine.locations, &written)?, Removal::Removed);
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&settings)?)?;
    assert!(doc["env"].get(ENV_KEY).is_none());
    Ok(())
}

#[test]
fn uninstall_with_nothing_recorded_touches_nothing() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    assert_eq!(uninstall(&machine.locations, &[])?, Removal::NotThere);
    assert!(!machine.locations.claude_settings().exists());
    Ok(())
}
