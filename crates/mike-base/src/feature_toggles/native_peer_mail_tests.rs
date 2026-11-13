use super::*;

#[test]
fn mod_lesson_delivery_defaults_off_and_applies_immediately() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "mod_lesson_delivery"), Some(false));
    let meta = TOGGLE_META
        .iter()
        .find(|m| m.key == "mod_lesson_delivery")
        .ok_or_else(|| anyhow::anyhow!("mod_lesson_delivery must have toggle meta"))?;
    assert_eq!(meta.scope, ToggleScope::Global);
    assert_eq!(meta.applies, ToggleApplies::Immediate);
    let on = set_toggle_enabled(&t, "mod_lesson_delivery", true)
        .ok_or_else(|| anyhow::anyhow!("set_toggle_enabled must return Some"))?;
    assert_eq!(get_toggle_enabled(&on, "mod_lesson_delivery"), Some(true));
    Ok(())
}

#[test]
fn native_peer_mail_defaults_off_and_applies_immediately() -> anyhow::Result<()> {
    let t = FeatureToggles::default();
    assert_eq!(get_toggle_enabled(&t, "native_peer_mail"), Some(false));
    let meta = TOGGLE_META
        .iter()
        .find(|m| m.key == "native_peer_mail")
        .ok_or_else(|| anyhow::anyhow!("native_peer_mail must have toggle meta"))?;
    assert_eq!(meta.scope, ToggleScope::Global);
    assert_eq!(meta.applies, ToggleApplies::Immediate);
    let on = set_toggle_enabled(&t, "native_peer_mail", true).ok_or_else(|| {
        anyhow::anyhow!("set_toggle_enabled must return Some for native_peer_mail")
    })?;
    assert_eq!(get_toggle_enabled(&on, "native_peer_mail"), Some(true));
    Ok(())
}
