use std::path::PathBuf;

use super::error::ClaudeCodeError;
use super::locations::Locations;
use super::mcp::{self, Registration};
use super::route::{self, Route};
use super::settings::{self, AUTO_TRUST_KEY, FUNCTION_KEY};
use super::shell::{self, Shell};
use super::{apply, mods};

#[derive(Debug, Clone)]
pub(crate) struct ShellStatus {
    pub(crate) shell: Shell,
    pub(crate) rc_path: PathBuf,
    pub(crate) installed: bool,
    pub(crate) block: Result<bool, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ClaudeCodeStatus {
    pub(crate) registration: Result<Registration, String>,
    pub(crate) route: Option<Route>,
    pub(crate) function_name: Option<String>,
    pub(crate) shells: Vec<ShellStatus>,
    pub(crate) recorded_mods: Vec<String>,
    pub(crate) listed_mods: Result<Vec<String>, String>,
    pub(crate) auto_trust: Option<bool>,
}

fn text(error: ClaudeCodeError) -> String {
    error.to_string()
}

pub(crate) fn gather(locations: &Locations) -> ClaudeCodeStatus {
    let config = &locations.config_file;
    let shells = Shell::ALL
        .into_iter()
        .map(|shell| {
            let rc_path = shell.rc_path(locations, std::env::consts::OS);
            ShellStatus {
                installed: shell.installed(locations.path_var.as_deref()),
                block: shell::has_block(&rc_path).map_err(text),
                rc_path,
                shell,
            }
        })
        .collect();
    ClaudeCodeStatus {
        registration: mcp::inspect(locations).map_err(text),
        route: route::stored(config).ok().flatten(),
        function_name: settings::read(config, FUNCTION_KEY).ok().flatten(),
        shells,
        recorded_mods: apply::recorded_mods(locations).unwrap_or_default(),
        listed_mods: mods::listed(locations).map_err(text),
        auto_trust: settings::read_bool(config, AUTO_TRUST_KEY).ok().flatten(),
    }
}

impl ClaudeCodeStatus {
    pub(crate) fn mods_in_place(&self) -> bool {
        match &self.listed_mods {
            Ok(listed) => {
                !self.recorded_mods.is_empty()
                    && self.recorded_mods.iter().all(|dir| listed.contains(dir))
            }
            Err(_) => false,
        }
    }

    pub(crate) fn render(&self, locations: &Locations) -> String {
        let mut out = String::new();
        out.push_str(&self.render_mcp());
        out.push_str(&match self.route {
            Some(route) => format!("route: {} ({})\n", route.as_str(), route.describe()),
            None => "route: not chosen (`mike claude-code route sidecar` or `plain`)\n".to_string(),
        });
        out.push_str(&self.render_shells(locations));
        out.push_str(&if self.mods_in_place() {
            format!(
                "mods: {} folders listed in CLAUDE_CODE_PLUGIN_DIRS\n",
                self.recorded_mods.len()
            )
        } else {
            "mods: not installed by mike (`mike claude-code mods`)\n".to_string()
        });
        out.push_str(&match self.auto_trust {
            Some(true) => "folder trust: `mike claude` marks the folder as trusted\n".to_string(),
            Some(false) => "folder trust: left to Claude Code's own dialog\n".to_string(),
            None => "folder trust: not chosen (`mike claude-code trust on` or `off`)\n".to_string(),
        });
        out
    }

    fn render_mcp(&self) -> String {
        match &self.registration {
            Err(why) => format!("mcp: {why}\n"),
            Ok(r) if r.user_scope() => format!(
                "mcp: registered with {}{}\n",
                r.command.as_deref().unwrap_or_default(),
                if r.command_exists {
                    ""
                } else {
                    " (this file does not exist)"
                }
            ),
            Ok(r) if r.in_a_project => "mcp: registered for a project only\n".to_string(),
            Ok(_) => "mcp: not registered (`mike claude-code mcp`)\n".to_string(),
        }
    }

    fn render_shells(&self, locations: &Locations) -> String {
        let name = self.function_name.as_deref().unwrap_or("cc");
        let mut out = String::new();
        for s in &self.shells {
            let file = apply::shown(&s.rc_path, locations);
            let state = match (&s.block, s.installed) {
                (Ok(true), _) => format!("`{name}` function present in {file}"),
                (Ok(false), true) => format!("no mike block in {file}"),
                (Ok(false), false) => "not installed on this machine".to_string(),
                (Err(why), _) => why.clone(),
            };
            out.push_str(&format!("shell {}: {state}\n", s.shell.name()));
        }
        out
    }
}
