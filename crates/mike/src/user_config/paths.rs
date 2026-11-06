use std::path::{Path, PathBuf};

use super::UserConfigError;

const APP_DIR: &str = "mike";
const CONFIG_FILE_NAME: &str = "config.toml";

fn absolute(value: Option<&str>) -> Option<&Path> {
    value
        .filter(|v| !v.is_empty())
        .map(Path::new)
        .filter(|p| p.is_absolute())
}

pub(crate) fn config_dir_from(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, UserConfigError> {
    let base = match absolute(xdg_config_home) {
        Some(xdg) => xdg.to_path_buf(),
        None => PathBuf::from(
            home.filter(|h| !h.is_empty())
                .ok_or(UserConfigError::NoConfigDir)?,
        )
        .join(".config"),
    };
    Ok(base.join(APP_DIR))
}

pub(crate) fn config_file_from(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, UserConfigError> {
    Ok(config_dir_from(xdg_config_home, home)?.join(CONFIG_FILE_NAME))
}

pub(crate) fn default_config_file() -> Result<PathBuf, UserConfigError> {
    let config_home = crate::config::env_text(crate::config::paths::CONFIG_HOME_ENV)
        .filter(|value| Path::new(value).is_absolute())
        .or_else(|| crate::config::env_text("XDG_CONFIG_HOME"));
    let file = config_file_from(
        config_home.as_deref(),
        crate::config::env_text("HOME").as_deref(),
    )?;
    crate::config::paths::live_install_guard::refuse(&file);
    Ok(file)
}

pub(crate) fn state_dir_from(
    xdg_state_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, UserConfigError> {
    crate::machine_identity::state_dir(xdg_state_home, home)
        .map_err(|_| UserConfigError::NoStateDir)
}

pub(crate) fn state_dir() -> Result<PathBuf, UserConfigError> {
    let dir = state_dir_from(
        crate::config::env_text("XDG_STATE_HOME").as_deref(),
        crate::config::env_text("HOME").as_deref(),
    )?;
    crate::config::paths::live_install_guard::refuse(&dir);
    Ok(dir)
}
