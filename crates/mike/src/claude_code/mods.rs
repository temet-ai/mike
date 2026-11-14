use std::path::{Path, PathBuf};

use serde_json::Value;

use super::atomic::{Edited, Removal, edit_object, object_member, read_object};
use super::error::ClaudeCodeError;
use super::locations::Locations;

pub(crate) const ENV_KEY: &str = "CLAUDE_CODE_PLUGIN_DIRS";
const OPT_IN_ONLY: &str = "wiretap";

fn is_mods_root(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .path()
                .join(".claude-plugin")
                .join("plugin.json")
                .is_file()
        })
    })
}

pub(crate) fn package_dirs(exe: &Path) -> Vec<PathBuf> {
    let stable = crate::service::stable_binary_path(exe).ok();
    let real = std::fs::canonicalize(exe).ok();
    [stable, real]
        .into_iter()
        .flatten()
        .flat_map(|bin| {
            bin.parent()
                .into_iter()
                .flat_map(|directory| {
                    [Some(directory), directory.parent()]
                        .into_iter()
                        .flatten()
                        .map(|prefix| prefix.join("share").join("mike"))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(crate) fn find_source(exe: Option<&Path>, cwd: Option<&Path>) -> Option<PathBuf> {
    let package_roots = exe
        .into_iter()
        .flat_map(package_dirs)
        .map(|share| share.join("mods"));
    let checkout_roots = exe
        .and_then(|exe| std::fs::canonicalize(exe).ok())
        .into_iter()
        .chain(cwd.map(Path::to_path_buf))
        .flat_map(|start| {
            start
                .ancestors()
                .map(|dir| dir.join("mods"))
                .collect::<Vec<_>>()
        });
    package_roots
        .chain(checkout_roots)
        .find(|root| is_mods_root(root))
}

pub(crate) fn find_for_process() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    find_source(exe.as_deref(), cwd.as_deref())
}

pub(crate) fn list(root: &Path, with_wiretap: bool) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| dir.join(".claude-plugin").join("plugin.json").is_file())
        .filter(|dir| {
            with_wiretap || dir.file_name().and_then(std::ffi::OsStr::to_str) != Some(OPT_IN_ONLY)
        })
        .collect();
    found.sort();
    found
}

fn same_directory(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub(crate) struct Merged {
    pub(crate) value: String,
    pub(crate) added: Vec<String>,
}

pub(crate) fn merged(existing: &str, ours: &[String], previous: &[String], root: &Path) -> Merged {
    let root_prefix = format!("{}/", root.display());
    let kept: Vec<&str> = existing
        .split(':')
        .filter(|entry| !entry.is_empty())
        .filter(|entry| {
            !previous.iter().any(|p| p == entry)
                && !entry.starts_with(&root_prefix)
                && !ours.iter().any(|o| o == entry)
        })
        .collect();
    let added: Vec<String> = ours
        .iter()
        .filter(|ours| !kept.iter().any(|kept| same_directory(kept, ours)))
        .cloned()
        .collect();
    let value = added
        .iter()
        .map(String::as_str)
        .chain(kept)
        .collect::<Vec<_>>()
        .join(":");
    Merged { value, added }
}

pub(crate) fn install(
    locations: &Locations,
    root: &Path,
    dirs: &[PathBuf],
    previous: &[String],
) -> Result<(Edited, Vec<String>), ClaudeCodeError> {
    let path = locations.claude_settings();
    let ours: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
    edit_object(&path, |settings| {
        let env = object_member(settings, "env", &path)?;
        let existing = env.get(ENV_KEY).and_then(Value::as_str).unwrap_or_default();
        let result = merged(existing, &ours, previous, root);
        if result.value.is_empty() {
            env.remove(ENV_KEY);
        } else {
            env.insert(ENV_KEY.to_string(), Value::String(result.value));
        }
        Ok(result.added)
    })
}

pub(crate) fn uninstall(
    locations: &Locations,
    written: &[String],
) -> Result<Removal, ClaudeCodeError> {
    if written.is_empty() {
        return Ok(Removal::NotThere);
    }
    let path = locations.claude_settings();
    let (_, removal) = edit_object(&path, |settings| {
        let Some(env) = settings.get_mut("env").and_then(Value::as_object_mut) else {
            return Ok(Removal::NotThere);
        };
        let Some(existing) = env.get(ENV_KEY).and_then(Value::as_str) else {
            return Ok(Removal::NotThere);
        };
        let kept: Vec<&str> = existing
            .split(':')
            .filter(|entry| !entry.is_empty() && !written.iter().any(|w| w == entry))
            .collect();
        if kept.len() == existing.split(':').filter(|e| !e.is_empty()).count() {
            return Ok(Removal::NotThere);
        }
        if kept.is_empty() {
            env.remove(ENV_KEY);
        } else {
            let value = kept.join(":");
            env.insert(ENV_KEY.to_string(), Value::String(value));
        }
        Ok(Removal::Removed)
    })?;
    Ok(removal)
}

pub(crate) fn listed(locations: &Locations) -> Result<Vec<String>, ClaudeCodeError> {
    let settings = read_object(&locations.claude_settings())?;
    Ok(settings
        .get("env")
        .and_then(|env| env.get(ENV_KEY))
        .and_then(Value::as_str)
        .map(|value| {
            value
                .split(':')
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}
