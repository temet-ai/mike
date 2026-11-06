use anyhow::Result;

use super::paths::{CONFIG_HOME_ENV, InstallationPaths};
use super::test_env::{EnvVarGuard, env_lock};

#[test]
fn a_seat_with_its_own_xdg_config_home_resolves_the_operators_installation() -> Result<()> {
    let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let temp = tempfile::tempdir()?;
    let operator = temp.path().join("operator");
    let seat = temp.path().join("seats/project/config");
    let _xdg = EnvVarGuard::set("XDG_CONFIG_HOME", &operator.to_string_lossy());
    let _mike = EnvVarGuard::unset(CONFIG_HOME_ENV);
    let daemon = InstallationPaths::resolve(None)?;
    let _seat_xdg = EnvVarGuard::set("XDG_CONFIG_HOME", &seat.to_string_lossy());
    assert_ne!(
        InstallationPaths::resolve(None)?.installation_id,
        daemon.installation_id
    );
    let _pinned = EnvVarGuard::set(CONFIG_HOME_ENV, &operator.to_string_lossy());
    assert_eq!(InstallationPaths::resolve(None)?, daemon);
    Ok(())
}

#[test]
fn a_relative_mike_config_home_is_ignored() -> Result<()> {
    let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let temp = tempfile::tempdir()?;
    let _xdg = EnvVarGuard::set("XDG_CONFIG_HOME", &temp.path().to_string_lossy());
    let _mike = EnvVarGuard::set(CONFIG_HOME_ENV, "relative/config");
    let paths = InstallationPaths::resolve(None)?;
    assert_eq!(paths.config_dir, temp.path().join("mike"));
    Ok(())
}

#[test]
fn the_seat_config_file_used_by_doctor_follows_mike_config_home() -> Result<()> {
    let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let temp = tempfile::tempdir()?;
    let operator = temp.path().join("operator");
    let _xdg = EnvVarGuard::set(
        "XDG_CONFIG_HOME",
        &temp.path().join("seat").to_string_lossy(),
    );
    let _mike = EnvVarGuard::set(CONFIG_HOME_ENV, &operator.to_string_lossy());
    let file = crate::user_config::paths::default_config_file()?;
    assert_eq!(file, operator.join("mike/config.toml"));
    Ok(())
}
