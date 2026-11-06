use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use clap::ValueEnum;

use super::atomic::{Edited, Removal, read_text, write_text};
use super::error::ClaudeCodeError;
use super::locations::Locations;
use super::route::Route;
use crate::launch::cli::locate_executable;

pub(crate) const BEGIN: &str = "# >>> mike >>>";
pub(crate) const END: &str = "# <<< mike <<<";
const RC_MODE: u32 = 0o644;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    pub(crate) const ALL: [Shell; 3] = [Shell::Zsh, Shell::Bash, Shell::Fish];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Shell::Zsh => "zsh",
            Shell::Bash => "bash",
            Shell::Fish => "fish",
        }
    }

    pub(crate) fn rc_path(self, locations: &Locations, os: &str) -> PathBuf {
        match self {
            Shell::Zsh => locations
                .zdotdir
                .as_ref()
                .unwrap_or(&locations.home)
                .join(".zshrc"),
            Shell::Bash if os == "macos" => locations.home.join(".bash_profile"),
            Shell::Bash => locations.home.join(".bashrc"),
            Shell::Fish => locations
                .xdg_config_home
                .clone()
                .unwrap_or_else(|| locations.home.join(".config"))
                .join("fish")
                .join("config.fish"),
        }
    }

    pub(crate) fn from_login_shell(shell_var: Option<&str>) -> Option<Self> {
        let name = Path::new(shell_var?).file_name()?.to_str()?;
        Self::ALL.into_iter().find(|shell| shell.name() == name)
    }

    pub(crate) fn installed(self, path_var: Option<&OsStr>) -> bool {
        locate_executable(self.name(), path_var).is_some()
    }
}

pub(crate) struct Function<'a> {
    pub(crate) name: &'a str,
    pub(crate) route: Route,
    pub(crate) proxy_addr: &'a str,
    pub(crate) mike: &'a str,
}

fn quoted(word: &str) -> String {
    if word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+'))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

pub(crate) fn mike_command(locations: &Locations, exe: Option<&Path>) -> String {
    if locate_executable("mike", locations.path_var.as_deref()).is_some() {
        return "mike".to_string();
    }
    exe.and_then(|exe| crate::service::stable_binary_path(exe).ok())
        .map_or_else(
            || "mike".to_string(),
            |path| quoted(&path.display().to_string()),
        )
}

pub(crate) fn render_block(shell: Shell, function: &Function<'_>) -> String {
    let name = function.name;
    let body = match (shell, function.route) {
        (Shell::Fish, Route::Sidecar) => format!(
            "function {name}\n    command {} claude $argv\nend",
            function.mike
        ),
        (Shell::Fish, Route::Plain) => format!(
            "function {name}\n    env ANTHROPIC_BASE_URL=http://{} claude $argv\nend",
            function.proxy_addr
        ),
        (_, Route::Sidecar) => {
            format!("{name}() {{\n  command {} claude \"$@\"\n}}", function.mike)
        }
        (_, Route::Plain) => format!(
            "{name}() {{\n  ANTHROPIC_BASE_URL=http://{} command claude \"$@\"\n}}",
            function.proxy_addr
        ),
    };
    format!(
        "{BEGIN}\n# Written by mike setup. `mike uninstall` removes this block.\n{body}\n{END}\n"
    )
}

fn block_span(text: &str) -> Result<Option<(usize, usize)>, ()> {
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let end_of_line = offset + line.len();
        match (line.trim(), start) {
            (BEGIN, None) => start = Some(offset),
            (END, Some(begin)) => return Ok(Some((begin, end_of_line))),
            _ => {}
        }
        offset = end_of_line;
    }
    if start.is_some() { Err(()) } else { Ok(None) }
}

pub(crate) fn with_block(path: &Path, text: &str, block: &str) -> Result<String, ClaudeCodeError> {
    let broken = || ClaudeCodeError::BrokenBlock {
        path: path.to_path_buf(),
        marker: BEGIN,
    };
    match block_span(text).map_err(|()| broken())? {
        Some((start, end)) => Ok(format!("{}{block}{}", &text[..start], &text[end..])),
        None if text.is_empty() => Ok(block.to_string()),
        None => {
            let separator = if text.ends_with('\n') { "\n" } else { "\n\n" };
            Ok(format!("{text}{separator}{block}"))
        }
    }
}

pub(crate) fn without_block(path: &Path, text: &str) -> Result<Option<String>, ClaudeCodeError> {
    let span = block_span(text).map_err(|()| ClaudeCodeError::BrokenBlock {
        path: path.to_path_buf(),
        marker: BEGIN,
    })?;
    Ok(span.map(|(start, end)| {
        let before = &text[..start];
        let after = &text[end..];
        let before = if after.is_empty() && before.ends_with("\n\n") {
            &before[..before.len() - 1]
        } else {
            before
        };
        format!("{before}{after}")
    }))
}

pub(crate) fn install(path: &Path, block: &str) -> Result<Edited, ClaudeCodeError> {
    let text = read_text(path)?.unwrap_or_default();
    let updated = with_block(path, &text, block)?;
    if updated == text {
        return Ok(Edited::Unchanged);
    }
    write_text(path, &updated, RC_MODE)?;
    Ok(Edited::Written)
}

pub(crate) fn uninstall(path: &Path) -> Result<Removal, ClaudeCodeError> {
    let Some(text) = read_text(path)? else {
        return Ok(Removal::NotThere);
    };
    match without_block(path, &text)? {
        None => Ok(Removal::NotThere),
        Some(updated) => {
            write_text(path, &updated, RC_MODE)?;
            Ok(Removal::Removed)
        }
    }
}

pub(crate) fn has_block(path: &Path) -> Result<bool, ClaudeCodeError> {
    let Some(text) = read_text(path)? else {
        return Ok(false);
    };
    Ok(block_span(&text)
        .map_err(|()| ClaudeCodeError::BrokenBlock {
            path: path.to_path_buf(),
            marker: BEGIN,
        })?
        .is_some())
}
