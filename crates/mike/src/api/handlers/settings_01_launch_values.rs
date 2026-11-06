use super::AppState;

pub(super) fn launch_resolved_values(
    state: &AppState,
) -> serde_json::Map<String, serde_json::Value> {
    launch_config_as_flat_map(&state.config.launch)
}

fn launch_config_as_flat_map(
    launch: &crate::config::LaunchConfig,
) -> serde_json::Map<String, serde_json::Value> {
    serde_json::to_value(launch)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LaunchConfig;

    #[test]
    fn every_launch_settings_meta_key_resolves_from_the_flat_map() {
        let map = launch_config_as_flat_map(&LaunchConfig::default());
        for m in crate::settings::SETTINGS_META
            .iter()
            .filter(|m| m.key.starts_with("launch_"))
        {
            assert!(
                map.contains_key(m.key),
                "{} missing from LaunchConfig's flat serialisation",
                m.key
            );
        }
    }

    #[test]
    fn resolved_backend_matches_the_configured_field_not_the_compiled_default() {
        let mut launch = LaunchConfig::default();
        assert_eq!(launch.backend, "none", "sanity: default is 'none'");
        launch.backend = "herdr".to_string();
        let map = launch_config_as_flat_map(&launch);
        assert_eq!(
            map.get("launch_backend").and_then(|v| v.as_str()),
            Some("herdr")
        );
    }

    #[test]
    fn non_launch_key_is_absent_from_the_flat_map() {
        let map = launch_config_as_flat_map(&LaunchConfig::default());
        assert!(!map.contains_key("reminders.inject_lessons"));
    }
}
