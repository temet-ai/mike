use std::path::{Path, PathBuf};

use super::atomic::{Edited, Removal};
use super::error::ClaudeCodeError;
use super::locations::Locations;
use super::route::{self, Route};
use super::settings::{self, AUTO_TRUST_KEY, FUNCTION_KEY, MODS_KEY};
use super::shell::{self, Function, Shell};
use super::{mcp, mods, probe, shell_name};
use crate::finding::Finding;

pub(crate) fn shown(path: &Path, locations: &Locations) -> String {
    match path.strip_prefix(&locations.home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

pub(crate) async fn connect_mcp(
    locations: &Locations,
    inspect_url: &str,
    check: bool,
) -> Result<Vec<Finding>, ClaudeCodeError> {
    let binary = mcp::locate_for_process(locations).ok_or(ClaudeCodeError::NoMcpBinary)?;
    let edited = mcp::register(locations, &binary, inspect_url)?;
    let file = shown(&locations.claude_json(), locations);
    let verb = match edited {
        Edited::Written => "Registered",
        Edited::Unchanged => "Already registered",
    };
    let mut findings = vec![Finding::ok(format!(
        "{verb} `mike` in {file} with the command {} and MIKE_INSPECT_URL={inspect_url}.",
        binary.display()
    ))];
    if check {
        findings.push(
            match probe::tool_count(&binary, inspect_url, probe::PROBE_WAIT).await {
                Ok(count) => Finding::ok(format!(
                    "Started {} and it listed {count} tools.",
                    binary.display()
                )),
                Err(e) => Finding::failed(format!("Registered, but {e}")),
            },
        );
    }
    Ok(findings)
}

pub(crate) fn disconnect_mcp(locations: &Locations) -> Result<Vec<Finding>, ClaudeCodeError> {
    let file = shown(&locations.claude_json(), locations);
    Ok(vec![match mcp::unregister(locations)? {
        Removal::Removed => Finding::ok(format!("Removed the `mike` MCP entry from {file}.")),
        Removal::NotThere => Finding::ok(format!("{file} has no `mike` MCP entry.")),
        Removal::NotOurs => Finding::ok(format!(
            "Left the `mike` entry in {file} alone, because it does not start mike-mcp."
        )),
    }])
}

pub(crate) fn choose_route(
    locations: &Locations,
    chosen: Route,
) -> Result<Vec<Finding>, ClaudeCodeError> {
    route::store(&locations.config_file, chosen)?;
    let stored = route::stored(&locations.config_file)?;
    Ok(vec![if stored == Some(chosen) {
        Finding::ok(format!(
            "Saved the route in {}: Claude Code reaches mike {}.",
            shown(&locations.config_file, locations),
            chosen.describe()
        ))
    } else {
        Finding::failed(format!(
            "Wrote the route, but reading {} back did not return `{}`.",
            shown(&locations.config_file, locations),
            chosen.as_str()
        ))
    }])
}

pub(crate) struct ShellRequest {
    pub(crate) name: String,
    pub(crate) shells: Vec<Shell>,
    pub(crate) route: Route,
    pub(crate) proxy_addr: String,
}

pub(crate) fn add_function(
    locations: &Locations,
    request: &ShellRequest,
    exe: Option<&Path>,
) -> Result<Vec<Finding>, ClaudeCodeError> {
    if request.shells.is_empty() {
        return Err(ClaudeCodeError::Other(
            "none of zsh, bash or fish was found on this machine. Name the shell with --shell"
                .to_string(),
        ));
    }
    shell_name::validate(&request.name, locations.path_var.as_deref())?;
    let mike = shell::mike_command(locations, exe);
    let function = Function {
        name: &request.name,
        route: request.route,
        proxy_addr: &request.proxy_addr,
        mike: &mike,
    };
    let mut findings = Vec::new();
    for chosen in &request.shells {
        let path = chosen.rc_path(locations, std::env::consts::OS);
        let block = shell::render_block(*chosen, &function);
        let edited = shell::install(&path, &block)?;
        let present = shell::has_block(&path)?;
        let file = shown(&path, locations);
        findings.push(match (edited, present) {
            (_, false) => {
                Finding::failed(format!("Wrote {file}, but the marked block is not in it."))
            }
            (Edited::Written, true) => Finding::ok(format!(
                "Wrote the `{}` function for {} between the mike markers in {file}.",
                request.name,
                chosen.name()
            )),
            (Edited::Unchanged, true) => Finding::ok(format!(
                "{file} already has the `{}` function for {}.",
                request.name,
                chosen.name()
            )),
        });
    }
    settings::write_text(&locations.config_file, FUNCTION_KEY, &request.name)?;
    Ok(findings)
}

pub(crate) fn remove_functions(locations: &Locations) -> Result<Vec<Finding>, ClaudeCodeError> {
    let mut findings = Vec::new();
    for chosen in Shell::ALL {
        let path = chosen.rc_path(locations, std::env::consts::OS);
        let file = shown(&path, locations);
        findings.push(match shell::uninstall(&path)? {
            Removal::Removed => Finding::ok(format!("Removed the mike block from {file}.")),
            Removal::NotThere | Removal::NotOurs => {
                Finding::ok(format!("{file} has no mike block."))
            }
        });
    }
    Ok(findings)
}

pub(crate) fn add_mods(
    locations: &Locations,
    root: &Path,
    with_wiretap: bool,
) -> Result<Vec<Finding>, ClaudeCodeError> {
    let dirs: Vec<PathBuf> = mods::list(root, with_wiretap);
    if dirs.is_empty() {
        return Err(ClaudeCodeError::NoModsSource);
    }
    let previous = recorded_mods(locations)?;
    let (edited, added) = mods::install(locations, root, &dirs, &previous)?;
    settings::write_text(&locations.config_file, MODS_KEY, &added.join(":"))?;
    let listed = mods::listed(locations)?;
    let missing: Vec<&String> = added.iter().filter(|dir| !listed.contains(dir)).collect();
    let file = shown(&locations.claude_settings(), locations);
    Ok(vec![if !missing.is_empty() {
        Finding::failed(format!(
            "Wrote {file}, but CLAUDE_CODE_PLUGIN_DIRS in it does not list {}.",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    } else if edited == Edited::Unchanged {
        Finding::ok(format!(
            "CLAUDE_CODE_PLUGIN_DIRS in {file} already lists the mods in {}.",
            root.display()
        ))
    } else {
        Finding::ok(format!(
            "Set CLAUDE_CODE_PLUGIN_DIRS in {file} to the {} mods in {}.",
            dirs.len(),
            root.display()
        ))
    }])
}

pub(crate) fn recorded_mods(locations: &Locations) -> Result<Vec<String>, ClaudeCodeError> {
    Ok(settings::read(&locations.config_file, MODS_KEY)?
        .map(|text| {
            text.split(':')
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

pub(crate) fn remove_mods(locations: &Locations) -> Result<Vec<Finding>, ClaudeCodeError> {
    let written = recorded_mods(locations)?;
    let file = shown(&locations.claude_settings(), locations);
    let finding = match mods::uninstall(locations, &written)? {
        Removal::Removed => Finding::ok(format!(
            "Removed the {} mod folders mike added to CLAUDE_CODE_PLUGIN_DIRS in {file}.",
            written.len()
        )),
        Removal::NotThere | Removal::NotOurs => {
            Finding::ok(format!("{file} lists no mod folders that mike added."))
        }
    };
    settings::remove(&locations.config_file, &[MODS_KEY])?;
    Ok(vec![finding])
}

pub(crate) fn choose_trust(
    locations: &Locations,
    trust: bool,
) -> Result<Vec<Finding>, ClaudeCodeError> {
    settings::write_bool(&locations.config_file, AUTO_TRUST_KEY, trust)?;
    let stored = settings::read_bool(&locations.config_file, AUTO_TRUST_KEY)?;
    let file = shown(&locations.config_file, locations);
    Ok(vec![if stored != Some(trust) {
        Finding::failed(format!(
            "Wrote {file}, but reading launch_auto_trust back failed."
        ))
    } else if trust {
        Finding::ok(format!(
            "Saved in {file}: `mike claude` marks the folder it starts in as trusted in ~/.claude.json."
        ))
    } else {
        Finding::ok(format!(
            "Saved in {file}: `mike claude` leaves folder trust to Claude Code's own dialog."
        ))
    }])
}
