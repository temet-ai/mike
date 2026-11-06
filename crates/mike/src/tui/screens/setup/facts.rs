use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use crate::db_provision::{Call, Runner};
use crate::launch::cli::locate_executable;

const VERSION_WAIT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Tool {
    Brew,
    Docker,
    Claude,
    Codex,
    Ollama,
    Herdr,
    Tmux,
    Zsh,
    Bash,
    Fish,
    Hyprctl,
    XdgTerminalExec,
    Pacman,
    Apt,
    Dnf,
}

impl Tool {
    pub(super) const ALL: [Tool; 15] = [
        Tool::Brew,
        Tool::Docker,
        Tool::Claude,
        Tool::Codex,
        Tool::Ollama,
        Tool::Herdr,
        Tool::Tmux,
        Tool::Zsh,
        Tool::Bash,
        Tool::Fish,
        Tool::Hyprctl,
        Tool::XdgTerminalExec,
        Tool::Pacman,
        Tool::Apt,
        Tool::Dnf,
    ];

    pub(super) fn binary(self) -> String {
        match self {
            Tool::Brew => "brew",
            Tool::Docker => "docker",
            Tool::Claude => return crate::claude::claude_binary(),
            Tool::Codex => "codex",
            Tool::Ollama => "ollama",
            Tool::Herdr => "herdr",
            Tool::Tmux => "tmux",
            Tool::Zsh => "zsh",
            Tool::Bash => "bash",
            Tool::Fish => "fish",
            Tool::Hyprctl => "hyprctl",
            Tool::XdgTerminalExec => "xdg-terminal-exec",
            Tool::Pacman => "pacman",
            Tool::Apt => "apt",
            Tool::Dnf => "dnf",
        }
        .to_string()
    }

    pub(super) fn display(self) -> &'static str {
        match self {
            Tool::Brew => "Homebrew",
            Tool::Docker => "Docker",
            Tool::Claude => "Claude Code",
            Tool::Codex => "Codex",
            Tool::Ollama => "Ollama",
            Tool::Herdr => "herdr",
            Tool::Tmux => "tmux",
            Tool::Zsh => "zsh",
            Tool::Bash => "bash",
            Tool::Fish => "fish",
            Tool::Hyprctl => "hyprctl",
            Tool::XdgTerminalExec => "xdg-terminal-exec",
            Tool::Pacman => "pacman",
            Tool::Apt => "apt",
            Tool::Dnf => "dnf",
        }
    }

    fn version_args(self) -> Option<&'static [&'static str]> {
        match self {
            Tool::Tmux => Some(&["-V"]),
            Tool::Hyprctl | Tool::XdgTerminalExec | Tool::Pacman | Tool::Apt | Tool::Dnf => None,
            _ => Some(&["--version"]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ToolFact {
    pub(super) path: PathBuf,
    pub(super) version: Option<String>,
}

impl ToolFact {
    pub(super) fn describe(&self, tool: Tool) -> String {
        match &self.version {
            Some(version) => format!("{}: {version} ({})", tool.display(), self.path.display()),
            None => format!("{}: {}", tool.display(), self.path.display()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct Facts {
    tools: BTreeMap<Tool, Option<ToolFact>>,
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

async fn read_version<R: Runner>(runner: &R, tool: Tool, path: &std::path::Path) -> Option<String> {
    let args = tool.version_args()?;
    let program = path.display().to_string();
    let call = Call::new(&program, args);
    let captured = tokio::time::timeout(VERSION_WAIT, runner.run(&call))
        .await
        .ok()?
        .ok()?;
    if !captured.success {
        return None;
    }
    first_line(&captured.stdout).or_else(|| first_line(&captured.stderr))
}

async fn look<R: Runner>(
    runner: &R,
    path_var: Option<&OsStr>,
    tool: Tool,
) -> (Tool, Option<ToolFact>) {
    let Some(path) = locate_executable(&tool.binary(), path_var) else {
        return (tool, None);
    };
    let version = read_version(runner, tool, &path).await;
    (tool, Some(ToolFact { path, version }))
}

impl Facts {
    pub(super) async fn ensure<R: Runner>(
        &mut self,
        runner: &R,
        path_var: Option<&OsStr>,
        wanted: &[Tool],
    ) {
        let missing: Vec<Tool> = wanted
            .iter()
            .copied()
            .filter(|tool| !self.tools.contains_key(tool))
            .collect();
        let looked = futures::future::join_all(missing.iter().map(|t| look(runner, path_var, *t)));
        for (tool, fact) in looked.await {
            self.tools.insert(tool, fact);
        }
    }

    pub(super) async fn refresh<R: Runner>(
        &mut self,
        runner: &R,
        path_var: Option<&OsStr>,
        wanted: &[Tool],
    ) {
        for tool in wanted {
            self.tools.remove(tool);
        }
        self.ensure(runner, path_var, wanted).await;
    }

    pub(super) fn get(&self, tool: Tool) -> Option<&ToolFact> {
        self.tools.get(&tool).and_then(Option::as_ref)
    }

    pub(super) fn has(&self, tool: Tool) -> bool {
        self.get(tool).is_some()
    }

    pub(super) fn found_lines(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter_map(|(tool, fact)| fact.as_ref().map(|f| f.describe(*tool)))
            .collect()
    }

    pub(super) fn package_manager(&self) -> Option<Tool> {
        [Tool::Brew, Tool::Pacman, Tool::Apt, Tool::Dnf]
            .into_iter()
            .find(|tool| self.has(*tool))
    }

    pub(super) fn not_found(&self) -> Vec<&'static str> {
        self.tools
            .iter()
            .filter(|(_, fact)| fact.is_none())
            .map(|(tool, _)| tool.display())
            .collect()
    }
}
