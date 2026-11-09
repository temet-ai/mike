use anyhow::Result;

use super::paths::{CONFIG_HOME_ENV, InstallationPaths};
use crate::environment::{MapEnvironment, lookup};

#[test]
fn a_seat_with_its_own_xdg_config_home_resolves_the_operators_installation() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let operator = temp.path().join("operator");
    let seat = temp.path().join("seats/project/config");
    let daemon_env =
        MapEnvironment::isolated().with("XDG_CONFIG_HOME", &operator.to_string_lossy());
    let daemon = InstallationPaths::resolve_in(None, &lookup(&daemon_env))?;
    let seat_env = daemon_env.with("XDG_CONFIG_HOME", &seat.to_string_lossy());
    assert_ne!(
        InstallationPaths::resolve_in(None, &lookup(&seat_env))?.installation_id,
        daemon.installation_id
    );
    let pinned_env = seat_env.with(CONFIG_HOME_ENV, &operator.to_string_lossy());
    assert_eq!(
        InstallationPaths::resolve_in(None, &lookup(&pinned_env))?,
        daemon
    );
    Ok(())
}

#[test]
fn a_relative_mike_config_home_is_ignored() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let environment = MapEnvironment::isolated()
        .with("XDG_CONFIG_HOME", &temp.path().to_string_lossy())
        .with(CONFIG_HOME_ENV, "relative/config");
    let paths = InstallationPaths::resolve_in(None, &lookup(&environment))?;
    assert_eq!(paths.config_dir, temp.path().join("mike"));
    Ok(())
}

#[test]
fn the_seat_config_file_used_by_doctor_follows_mike_config_home() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let operator = temp.path().join("operator");
    let environment = MapEnvironment::isolated()
        .with(
            "XDG_CONFIG_HOME",
            &temp.path().join("seat").to_string_lossy(),
        )
        .with(CONFIG_HOME_ENV, &operator.to_string_lossy());
    let file = crate::user_config::paths::default_config_file_in(&environment)?;
    assert_eq!(file, operator.join("mike/config.toml"));
    Ok(())
}
