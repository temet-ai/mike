use mike::launch::fleet::{FleetDefinition, test_seams};
use serde_json::json;

#[test]
fn every_fleet_role_and_successor_defaults_to_a_noop_browser_helper() {
    for seat in [
        "worker-T1",
        "coordinator",
        "acceptance",
        "acceptance-2",
        "observer",
        "reference",
        "template-rust",
    ] {
        let settings = test_seams::seat_settings(&FleetDefinition::default(), seat);
        assert_eq!(settings["env"]["BROWSER"], "true", "{seat}");
        assert!(settings["env"]["TMPDIR"].is_string());
        assert!(settings["env"]["XDG_CONFIG_HOME"].is_string());
    }
}

#[test]
fn explicit_operator_browser_configuration_survives_the_fleet_defaults() {
    let definition = FleetDefinition {
        seat_settings: Some(json!({"env": {"BROWSER": "/operator/chosen-helper"}})),
        ..Default::default()
    };
    let settings = test_seams::seat_settings(&definition, "acceptance-2");
    assert_eq!(settings["env"]["BROWSER"], "/operator/chosen-helper");
    assert_eq!(settings["env"]["MIKE_MCP_PROFILE"], "acceptance");
}

#[cfg(unix)]
#[path = "fleet_environment/browser_guard.rs"]
mod browser_guard;

#[cfg(unix)]
#[path = "fleet_environment/browser_guard_cases.rs"]
mod browser_guard_cases;
