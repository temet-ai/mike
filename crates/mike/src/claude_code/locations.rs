use std::ffi::OsString;
use std::path::PathBuf;

use super::error::ClaudeCodeError;

#[derive(Debug, Clone)]
pub(crate) struct Locations {
    pub(crate) home: PathBuf,
    pub(crate) config_file: PathBuf,
    pub(crate) claude_config_dir: Option<PathBuf>,
    pub(crate) xdg_config_home: Option<PathBuf>,
    pub(crate) zdotdir: Option<PathBuf>,
    pub(crate) path_var: Option<OsString>,
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

impl Locations {
    pub(crate) fn from_environment() -> Result<Self, ClaudeCodeError> {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or(ClaudeCodeError::NoHome)?;
        Ok(Self {
            home,
            config_file: crate::user_config::resolved_config_file()?,
            claude_config_dir: absolute_env("CLAUDE_CONFIG_DIR"),
            xdg_config_home: absolute_env("XDG_CONFIG_HOME"),
            zdotdir: absolute_env("ZDOTDIR"),
            path_var: std::env::var_os("PATH"),
        })
    }

    pub(crate) fn claude_json(&self) -> PathBuf {
        match &self.claude_config_dir {
            Some(dir) => dir.join(".claude.json"),
            None => self.home.join(".claude.json"),
        }
    }

    pub(crate) fn claude_settings(&self) -> PathBuf {
        match &self.claude_config_dir {
            Some(dir) => dir.join("settings.json"),
            None => self.home.join(".claude").join("settings.json"),
        }
    }

    pub(crate) fn for_home(home: &std::path::Path, path_var: Option<OsString>) -> Self {
        Self {
            home: home.to_path_buf(),
            config_file: home.join(".config").join("mike").join("config.toml"),
            claude_config_dir: None,
            xdg_config_home: None,
            zdotdir: None,
            path_var,
        }
    }
}
