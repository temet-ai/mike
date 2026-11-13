use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::runner::Runner;
use crate::launch::{DEV_CHANNELS_FLAG, LaunchError, SplitLaunchArgs};

const SERVER: &str = "mike";
const SERVER_BIN: &str = "mike-mcp";
const MCP_CONFIG: &str = "--mcp-config";
const STRICT: &str = "--strict-mcp-config";
const ADD_SERVER: &str = "claude mcp add --scope user mike -- mike-mcp";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpPlan {
    Configured,
    Inject(String),
    Unknown(String),
}

pub struct Seat<'a> {
    pub config_path: &'a Path,
    pub folder: &'a Path,
    pub claude_args: &'a [String],
    pub server_bin: Option<&'a Path>,
    pub inspect_addr: Option<&'a str>,
}

fn executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn server_candidates() -> Vec<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(SERVER_BIN)));
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.join(SERVER_BIN));
    beside.into_iter().chain(on_path).collect()
}

pub fn find_server() -> Option<PathBuf> {
    server_candidates().into_iter().find(|p| executable(p))
}

pub fn tried_line() -> String {
    let places: Vec<String> = server_candidates()
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    format!("looked for {SERVER_BIN} at {}", places.join(", "))
}

fn names_server(servers_holder: &Value) -> bool {
    servers_holder
        .get("mcpServers")
        .and_then(|servers| servers.get(SERVER))
        .is_some()
}

fn project_entries<'a>(config: &'a Value, folder: &'a Path) -> impl Iterator<Item = &'a Value> {
    folder.ancestors().filter_map(move |dir| {
        config
            .get("projects")
            .and_then(|projects| projects.get(dir.to_string_lossy().as_ref()))
    })
}

fn disabled_in(config: &Value, folder: &Path) -> bool {
    project_entries(config, folder).any(|entry| {
        ["disabledMcpServers", "disabledMcpjsonServers"]
            .iter()
            .filter_map(|key| entry.get(key).and_then(Value::as_array))
            .flatten()
            .any(|name| name.as_str() == Some(SERVER))
    })
}

fn mcp_json_has_server(folder: &Path) -> bool {
    folder.ancestors().any(|dir| {
        std::fs::read_to_string(dir.join(".mcp.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|json| names_server(&json))
    })
}

fn config_value_names_server(value: &str) -> bool {
    let text = if value.trim_start().starts_with('{') {
        value.to_string()
    } else {
        std::fs::read_to_string(value).unwrap_or_default()
    };
    serde_json::from_str::<Value>(&text).is_ok_and(|json| names_server(&json))
}

fn passed_explicitly(claude_args: &[String]) -> bool {
    let mut iter = claude_args.iter().peekable();
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--mcp-config=") {
            if config_value_names_server(value) {
                return true;
            }
            continue;
        }
        if arg != MCP_CONFIG {
            continue;
        }
        while let Some(value) = iter.next_if(|next| !next.starts_with('-')) {
            if config_value_names_server(value) {
                return true;
            }
        }
    }
    false
}

fn read_config(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); whether the seat gets mike's MCP tools is not known",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(format!(
            "cannot read {} ({e}); whether the seat gets mike's MCP tools is not known",
            path.display()
        )),
    }
}

pub fn inline_config(server_bin: &Path, inspect_addr: Option<&str>) -> String {
    let env = inspect_addr.map_or_else(
        || json!({}),
        |addr| json!({"MIKE_INSPECT_URL": format!("http://{addr}")}),
    );
    json!({"mcpServers": {SERVER: {
        "type": "stdio",
        "command": server_bin.to_string_lossy(),
        "args": [],
        "env": env,
    }}})
    .to_string()
}

fn missing(seat: &Seat<'_>, strict: bool) -> Result<McpPlan, LaunchError> {
    let Some(bin) = seat.server_bin else {
        let why = if strict {
            format!(
                "the launch passes {STRICT} and none of its {MCP_CONFIG} values names `{SERVER}`"
            )
        } else {
            format!(
                "no `{SERVER}` MCP server is configured for {} (not in {}, its project entries or \
                 any .mcp.json above it)",
                seat.folder.display(),
                seat.config_path.display()
            )
        };
        return Err(LaunchError::Refused(format!(
            "{why}, and the config to pass it with {MCP_CONFIG} cannot be built: {}; the seat \
             would start without mike's tools (notes, mail, fleet). Install it (`make install`) \
             or run `{ADD_SERVER}`",
            tried_line()
        )));
    };
    Ok(McpPlan::Inject(inline_config(bin, seat.inspect_addr)))
}

pub fn plan(seat: &Seat<'_>) -> Result<McpPlan, LaunchError> {
    if passed_explicitly(seat.claude_args) {
        return Ok(McpPlan::Configured);
    }
    let strict = seat.claude_args.iter().any(|a| a == STRICT);
    if strict {
        return missing(seat, true);
    }
    let config = match read_config(seat.config_path) {
        Ok(config) => config,
        Err(why) => {
            return Ok(seat.server_bin.map_or(McpPlan::Unknown(why), |bin| {
                McpPlan::Inject(inline_config(bin, seat.inspect_addr))
            }));
        }
    };
    if disabled_in(&config, seat.folder) {
        return Err(LaunchError::Refused(format!(
            "the `{SERVER}` MCP server is disabled for {} in {}; the seat would start without \
             mike's tools. Enable it with /mcp in a session there, or remove `{SERVER}` from that \
             project's disabled list",
            seat.folder.display(),
            seat.config_path.display()
        )));
    }
    if names_server(&config)
        || project_entries(&config, seat.folder).any(names_server)
        || mcp_json_has_server(seat.folder)
    {
        return Ok(McpPlan::Configured);
    }
    missing(seat, false)
}

fn names_channels(claude_args: &[String]) -> bool {
    claude_args.iter().any(|a| {
        a == DEV_CHANNELS_FLAG
            || a.strip_prefix(DEV_CHANNELS_FLAG)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

fn drop_channels(split: &mut SplitLaunchArgs) {
    if !split.channels || names_channels(&split.claude_args) {
        return;
    }
    split.channels = false;
    eprintln!(
        "mike spawn: a server passed with {MCP_CONFIG} cannot carry channel push, so this seat \
         starts without it and reads mail on its next turn; `{ADD_SERVER}` gives it both"
    );
}

pub fn apply(
    runner: &Runner<'_>,
    folder: &Path,
    split: &mut SplitLaunchArgs,
) -> Result<(), LaunchError> {
    let seat = Seat {
        config_path: runner.trust.config_path(),
        folder,
        claude_args: &split.claude_args,
        server_bin: runner.mcp_server.as_deref(),
        inspect_addr: runner.inspect_addr.as_deref(),
    };
    let planned = plan(&seat)?;
    match planned {
        McpPlan::Configured => {}
        McpPlan::Unknown(why) => eprintln!("mike spawn: {why}"),
        McpPlan::Inject(config) => {
            eprintln!(
                "mike spawn: no `{SERVER}` MCP server is configured for {}; passing it with \
                 {MCP_CONFIG} so the seat has mike's tools",
                folder.display()
            );
            split
                .claude_args
                .insert(0, format!("{MCP_CONFIG}={config}"));
            drop_channels(split);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "mcp_preflight_tests.rs"]
pub mod tests;
