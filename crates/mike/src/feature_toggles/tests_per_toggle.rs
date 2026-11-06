use super::*;

#[test]
fn turn_guard_defaults_off() {
    assert!(!FeatureToggles::default().turn_guard);
    assert_eq!(
        get_toggle_enabled(&FeatureToggles::default(), "turn_guard"),
        Some(false)
    );
}

#[test]
fn turn_guard_meta_is_global_live_disable() {
    let meta = TOGGLE_META.iter().find(|m| m.key == "turn_guard");
    assert!(
        meta.is_some_and(
            |m| m.scope == ToggleScope::Global && m.applies == ToggleApplies::LiveDisable
        )
    );
}

#[test]
fn session_end_shadow_defaults_off() {
    assert!(!FeatureToggles::default().session_end_shadow);
    assert_eq!(
        get_toggle_enabled(&FeatureToggles::default(), "session_end_shadow"),
        Some(false)
    );
}

#[test]
fn session_end_shadow_meta_is_global_live_disable() {
    let meta = TOGGLE_META.iter().find(|m| m.key == "session_end_shadow");
    assert!(
        meta.is_some_and(
            |m| m.scope == ToggleScope::Global && m.applies == ToggleApplies::LiveDisable
        )
    );
}

#[test]
fn set_toggle_enabled_session_end_shadow_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    let on = set_toggle_enabled(&t, "session_end_shadow", true).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for session_end_shadow")
    })?;
    assert_eq!(get_toggle_enabled(&on, "session_end_shadow"), Some(true));
    assert_eq!(get_toggle_enabled(&t, "session_end_shadow"), Some(false));
    Ok(())
}

#[test]
fn launch_brief_defaults_off_and_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "launch_brief"), Some(false));
    let meta = TOGGLE_META.iter().find(|m| m.key == "launch_brief");
    assert!(meta.is_some_and(|m| m.scope == ToggleScope::Global));
    let on = set_toggle_enabled(&t, "launch_brief", true)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must return Some for launch_brief"))?;
    assert_eq!(get_toggle_enabled(&on, "launch_brief"), Some(true));
    Ok(())
}

#[test]
fn fleet_auto_start_defaults_on_and_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert!(t.fleet_auto_start);
    assert_eq!(get_toggle_enabled(&t, "fleet_auto_start"), Some(true));
    let meta = TOGGLE_META.iter().find(|m| m.key == "fleet_auto_start");
    assert!(meta.is_some_and(|m| m.scope == ToggleScope::Global));
    assert!(meta.is_some_and(|m| m.applies == ToggleApplies::Immediate));
    let off = set_toggle_enabled(&t, "fleet_auto_start", false).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for fleet_auto_start")
    })?;
    assert_eq!(get_toggle_enabled(&off, "fleet_auto_start"), Some(false));
    assert_eq!(get_toggle_enabled(&t, "fleet_auto_start"), Some(true));
    Ok(())
}

#[test]
fn change_record_toggles_default_off_and_round_trip() -> anyhow::Result<()> {
    for key in ["change_record", "change_record_tail"] {
        let t = FeatureToggles::default();
        assert_eq!(get_toggle_enabled(&t, key), Some(false));
        let meta = TOGGLE_META
            .iter()
            .find(|m| m.key == key)
            .ok_or_else(|| anyhow::anyhow!("{key} must be listed in TOGGLE_META"))?;
        assert_eq!(meta.scope, ToggleScope::Global);
        assert_eq!(meta.applies, ToggleApplies::Immediate);
        let on = set_toggle_enabled(&t, key, true)
            .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must know {key}"))?;
        assert_eq!(get_toggle_enabled(&on, key), Some(true));
        assert_eq!(get_toggle_enabled(&t, key), Some(false));
    }
    Ok(())
}

#[test]
fn repeated_notices_defaults_off_and_round_trips() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "repeated_notices"), Some(false));
    let meta = TOGGLE_META
        .iter()
        .find(|m| m.key == "repeated_notices")
        .ok_or_else(|| anyhow::anyhow!("repeated_notices must be listed in TOGGLE_META"))?;
    assert_eq!(meta.scope, ToggleScope::Session);
    assert_eq!(meta.applies, ToggleApplies::NewSessions);
    let on = set_toggle_enabled(&t, "repeated_notices", true)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must know repeated_notices"))?;
    assert_eq!(get_toggle_enabled(&on, "repeated_notices"), Some(true));
    let resolved = ResolvedToggles::from_overlay(&on, &crate::settings::SettingsOverlay::default());
    assert!(resolved.repeated_notices);
    Ok(())
}
