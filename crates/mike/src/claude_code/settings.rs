use std::path::Path;

use super::error::ClaudeCodeError;
use crate::user_config::{ConfigFile, SettingValue};

pub(crate) const ROUTE_KEY: &str = "claude_route";
pub(crate) const FUNCTION_KEY: &str = "claude_function";
pub(crate) const MODS_KEY: &str = "claude_mods";
pub(crate) const AUTO_TRUST_KEY: &str = "launch_auto_trust";
pub(crate) const SETUP_KEY: &str = "setup";

fn env_name(key: &str) -> String {
    crate::user_config::env_name_for(key)
}

pub(crate) fn read(config_file: &Path, key: &str) -> Result<Option<String>, ClaudeCodeError> {
    let file = ConfigFile::open(config_file)?;
    Ok(file
        .literal(&env_name(key))?
        .filter(|value| !value.trim().is_empty()))
}

pub(crate) fn write_text(
    config_file: &Path,
    key: &str,
    value: &str,
) -> Result<(), ClaudeCodeError> {
    let mut file = ConfigFile::open(config_file)?;
    file.set(key, SettingValue::Text(value))?;
    file.save()?;
    Ok(())
}

pub(crate) fn write_bool(
    config_file: &Path,
    key: &str,
    value: bool,
) -> Result<(), ClaudeCodeError> {
    let mut file = ConfigFile::open(config_file)?;
    file.set(key, SettingValue::Bool(value))?;
    file.save()?;
    Ok(())
}

pub(crate) fn remove(config_file: &Path, keys: &[&str]) -> Result<(), ClaudeCodeError> {
    let mut file = ConfigFile::open(config_file)?;
    for key in keys {
        file.remove(key);
    }
    file.save()?;
    Ok(())
}

pub(crate) fn read_bool(config_file: &Path, key: &str) -> Result<Option<bool>, ClaudeCodeError> {
    Ok(
        read(config_file, key)?.and_then(|text| match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        }),
    )
}
