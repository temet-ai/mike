use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::atomic::{Edited, Removal, edit_object, object_member, read_object};
use super::error::ClaudeCodeError;
use super::locations::Locations;
use crate::launch::cli::locate_executable;

pub(crate) const SERVER_NAME: &str = "mike";
pub(crate) const BINARY_NAME: &str = "mike-mcp";
const SERVERS_KEY: &str = "mcpServers";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Registration {
    pub(crate) command: Option<String>,
    pub(crate) command_exists: bool,
    pub(crate) inspect_url: Option<String>,
    pub(crate) in_a_project: bool,
}

impl Registration {
    pub(crate) fn user_scope(&self) -> bool {
        self.command.is_some()
    }
}

pub(crate) fn locate_binary(exe: Option<&Path>, path_var: Option<&OsStr>) -> Option<PathBuf> {
    let beside = |dir: &Path| {
        let candidate = dir.join(BINARY_NAME);
        candidate.is_file().then_some(candidate)
    };
    let from_exe = exe.and_then(|exe| {
        let stable = crate::service::stable_binary_path(exe).ok();
        stable
            .as_deref()
            .and_then(Path::parent)
            .and_then(beside)
            .or_else(|| exe.parent().and_then(beside))
    });
    let found = from_exe.or_else(|| locate_executable(BINARY_NAME, path_var))?;
    Some(crate::service::stable_binary_path(&found).unwrap_or(found))
}

pub(crate) fn locate_for_process(locations: &Locations) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    locate_binary(exe.as_deref(), locations.path_var.as_deref())
}

pub(crate) fn entry_for(binary: &Path, inspect_url: &str) -> Value {
    json!({
        "type": "stdio",
        "command": binary.display().to_string(),
        "args": [],
        "env": { "MIKE_INSPECT_URL": inspect_url },
    })
}

pub(crate) fn is_ours(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| {
            Path::new(command).file_name().and_then(OsStr::to_str) == Some(BINARY_NAME)
        })
}

pub(crate) fn register(
    locations: &Locations,
    binary: &Path,
    inspect_url: &str,
) -> Result<Edited, ClaudeCodeError> {
    let path = locations.claude_json();
    let entry = entry_for(binary, inspect_url);
    edit_object(&path, |root| {
        let servers = object_member(root, SERVERS_KEY, &path)?;
        servers.insert(SERVER_NAME.to_string(), entry);
        Ok(())
    })
    .map(|(edited, ())| edited)
}

pub(crate) fn unregister(locations: &Locations) -> Result<Removal, ClaudeCodeError> {
    let path = locations.claude_json();
    let (_, removal) = edit_object(&path, |root| {
        let Some(servers) = root.get_mut(SERVERS_KEY).and_then(Value::as_object_mut) else {
            return Ok(Removal::NotThere);
        };
        match servers.get(SERVER_NAME) {
            None => Ok(Removal::NotThere),
            Some(entry) if !is_ours(entry) => Ok(Removal::NotOurs),
            Some(_) => {
                servers.remove(SERVER_NAME);
                Ok(Removal::Removed)
            }
        }
    })?;
    Ok(removal)
}

fn project_registers(root: &Map<String, Value>) -> bool {
    root.get("projects")
        .and_then(Value::as_object)
        .is_some_and(|projects| {
            projects
                .values()
                .any(|p| p.pointer("/mcpServers/mike").is_some())
        })
}

pub(crate) fn inspect(locations: &Locations) -> Result<Registration, ClaudeCodeError> {
    let root = read_object(&locations.claude_json())?;
    let entry = root
        .get(SERVERS_KEY)
        .and_then(|servers| servers.get(SERVER_NAME));
    let command = entry
        .and_then(|e| e.get("command"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let command_exists = command
        .as_deref()
        .is_some_and(|c| locate_executable(c, locations.path_var.as_deref()).is_some());
    let inspect_url = entry
        .and_then(|e| e.pointer("/env/MIKE_INSPECT_URL"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(Registration {
        command,
        command_exists,
        inspect_url,
        in_a_project: project_registers(&root),
    })
}

pub(crate) fn inspect_url_for(config: &crate::config::ProxyConfig) -> String {
    format!(
        "http://{}",
        crate::doctor::connect_addr(&config.inspect_listen_addr)
    )
}
