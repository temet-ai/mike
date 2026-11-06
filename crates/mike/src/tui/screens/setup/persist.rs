use std::path::Path;

use crate::user_config::UserConfigError;

pub(super) fn persist_env_var_at(path: &Path, key: &str, value: &str) -> anyhow::Result<()> {
    if path.as_os_str().is_empty() {
        return Err(UserConfigError::NoConfigDir.into());
    }
    let name = key
        .strip_prefix("MIKE_")
        .unwrap_or(key)
        .to_ascii_lowercase();
    crate::setup::persistence::persist_boot_setting_at(path, &name, value)?;
    set_process_env(key, value);
    Ok(())
}

fn set_process_env(key: &str, value: &str) {
    unsafe {
        std::env::set_var(key, value);
    }
}

pub(super) fn names_an_environment_variable(input: &str) -> bool {
    let mut chars = input.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_uppercase() || first == '_')
        && input.len() >= 3
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}
