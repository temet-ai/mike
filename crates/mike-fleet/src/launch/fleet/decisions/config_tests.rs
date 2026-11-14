use serde_json::json;

use super::*;
use crate::launch::fleet::definition::FleetDefinition;

#[test]
fn a_definition_without_decisions_has_every_point_on() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&json!({})).map_err(anyhow::Error::msg)?;
    for point in [
        Point::StopCheck,
        Point::WakeCheck,
        Point::Triage,
        Point::CommitMessage,
        Point::Effort,
    ] {
        assert_eq!(def.decision_mode(point, None), Mode::On);
    }
    Ok(())
}

#[test]
fn stop_check_shadow_validates() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&json!({"decisions": {"stop_check": "shadow"}}))
        .map_err(anyhow::Error::msg)?;
    assert_eq!(def.decision_mode(Point::StopCheck, None), Mode::Shadow);
    assert_eq!(def.decision_mode(Point::WakeCheck, None), Mode::On);
    Ok(())
}

#[test]
fn a_misspelt_key_is_rejected_with_a_message_naming_it() {
    let found = FleetDefinition::from_value(&json!({"decisions": {"stop_chek": "on"}}));
    let message = found.err().unwrap_or_default();
    assert!(message.contains("stop_chek"), "{message}");
}

#[test]
fn an_unknown_mode_is_rejected() {
    let found = FleetDefinition::from_value(&json!({"decisions": {"triage": "maybe"}}));
    assert!(found.err().is_some_and(|m| m.contains("maybe")));
}

#[test]
fn a_worker_override_wins_over_the_fleet_for_its_own_discipline_only() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&json!({
        "decisions": {"wake_check": "on"},
        "workers": {
            "docs": {"decisions": {"wake_check": "off"}},
            "backend": {}
        }
    }))
    .map_err(anyhow::Error::msg)?;
    assert_eq!(def.decision_mode(Point::WakeCheck, Some("docs")), Mode::Off);
    assert_eq!(
        def.decision_mode(Point::WakeCheck, Some("backend")),
        Mode::On
    );
    assert_eq!(def.decision_mode(Point::WakeCheck, None), Mode::On);
    Ok(())
}

#[test]
fn a_worker_override_with_a_misspelt_key_is_rejected() {
    let found = FleetDefinition::from_value(&json!({
        "workers": {"docs": {"decisions": {"wake_chek": "off"}}}
    }));
    assert!(found.err().is_some_and(|m| m.contains("wake_chek")));
}

#[test]
fn a_default_decisions_block_is_left_out_of_the_stored_definition() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&json!({})).map_err(anyhow::Error::msg)?;
    let value = serde_json::to_value(&def)?;
    assert!(value.get("decisions").is_none());
    Ok(())
}

#[test]
fn point_names_match_the_decision_record_names() {
    assert_eq!(Point::StopCheck.name(), "fleet_stop_check");
    assert_eq!(Point::WakeCheck.name(), "fleet_wake_check");
    assert_eq!(Point::Triage.name(), "fleet_triage");
    assert_eq!(Point::CommitMessage.name(), "fleet_commit_message");
    assert_eq!(Point::Effort.name(), "fleet_effort");
}
