use super::*;

#[test]
fn internal_source_is_attributed_to_mike() {
    let source = ChangeSource::internal();
    assert_eq!(source.via, ChangeVia::Mike);
    assert!(source.actor.is_none());
}

#[test]
fn setting_json_carries_via_build_and_detail() -> anyhow::Result<()> {
    let source =
        ChangeSource::new(ChangeVia::Spa).with_detail("applies", serde_json::json!("immediate"));
    let json: serde_json::Value = serde_json::from_str(&source.to_setting_json(&current_build()))?;
    assert_eq!(json["via"], "spa");
    assert_eq!(json["build_sha"], current_build().sha);
    assert_eq!(json["detail"]["applies"], "immediate");
    Ok(())
}

#[test]
fn every_via_value_round_trips_through_serde() -> anyhow::Result<()> {
    for via in ["spa", "api", "startup", "promotion", "mike"] {
        let parsed: ChangeVia = serde_json::from_value(serde_json::json!(via))?;
        assert_eq!(serde_json::to_value(parsed)?, serde_json::json!(via));
    }
    Ok(())
}

#[test]
fn code_defaults_lists_every_setting_and_toggle() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::try_parse_from(["mike"])?;
    let defaults = code_defaults(&config);
    let settings = defaults["settings"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("settings is not an object"))?;
    assert_eq!(settings.len(), crate::settings::SETTINGS_META.len());
    let toggles = defaults["toggles"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("toggles is not an object"))?;
    assert_eq!(toggles.len(), crate::feature_toggles::TOGGLE_META.len());
    assert_eq!(
        toggles["dossier_builder"],
        serde_json::json!({"enabled": false})
    );
    Ok(())
}
