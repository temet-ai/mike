use std::path::Path;

use super::effective;
use crate::config::paths::CONFIG_HOME_ENV;
use crate::config::test_env::{EnvVarGuard, env_lock};
use crate::launch::fleet::FleetDefinition;

#[test]
fn a_seats_mike_commands_use_the_operators_config_folder_not_the_seats() {
    let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _mike = EnvVarGuard::unset(CONFIG_HOME_ENV);
    let _xdg = EnvVarGuard::set("XDG_CONFIG_HOME", "/operator/config");
    let scratch = Path::new("/state/mike/seats/holdfast");
    let settings = effective(&FleetDefinition::default(), scratch, "worker-1");
    assert_eq!(settings["env"][CONFIG_HOME_ENV], "/operator/config");
    assert_eq!(
        settings["env"]["XDG_CONFIG_HOME"],
        "/state/mike/seats/holdfast/config"
    );
}

#[test]
fn a_seat_spawned_from_a_seat_keeps_the_operators_config_folder() {
    let _lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _mike = EnvVarGuard::set(CONFIG_HOME_ENV, "/operator/config");
    let _xdg = EnvVarGuard::set("XDG_CONFIG_HOME", "/state/mike/seats/holdfast/config");
    let scratch = Path::new("/state/mike/seats/holdfast");
    let settings = effective(&FleetDefinition::default(), scratch, "worker-1");
    assert_eq!(settings["env"][CONFIG_HOME_ENV], "/operator/config");
}
