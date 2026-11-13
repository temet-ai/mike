use std::path::Path;

use super::effective_in;
use crate::config::paths::CONFIG_HOME_ENV;
use crate::environment::MapEnvironment;
use crate::launch::fleet::FleetDefinition;

#[test]
fn a_seats_mike_commands_use_the_operators_config_folder_not_the_seats() {
    let environment = MapEnvironment::new(&[("XDG_CONFIG_HOME", "/operator/config")]);
    let scratch = Path::new("/state/mike/seats/holdfast");
    let settings = effective_in(
        &FleetDefinition::default(),
        scratch,
        "worker-1",
        &environment,
    );
    assert_eq!(settings["env"][CONFIG_HOME_ENV], "/operator/config");
    assert_eq!(
        settings["env"]["XDG_CONFIG_HOME"],
        "/state/mike/seats/holdfast/config"
    );
}

#[test]
fn a_seat_spawned_from_a_seat_keeps_the_operators_config_folder() {
    let environment = MapEnvironment::new(&[
        (CONFIG_HOME_ENV, "/operator/config"),
        ("XDG_CONFIG_HOME", "/state/mike/seats/holdfast/config"),
    ]);
    let scratch = Path::new("/state/mike/seats/holdfast");
    let settings = effective_in(
        &FleetDefinition::default(),
        scratch,
        "worker-1",
        &environment,
    );
    assert_eq!(settings["env"][CONFIG_HOME_ENV], "/operator/config");
}
