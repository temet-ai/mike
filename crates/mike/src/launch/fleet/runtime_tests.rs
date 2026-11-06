use serde_json::json;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::{brief_version, check_definition, plan};
use crate::runtime::RuntimeKind;

#[test]
fn legacy_fleet_definitions_default_to_claude() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&super::tests::complete()).map_err(anyhow::Error::msg)?;
    assert_eq!(def.coordinator.runtime, RuntimeKind::Claude);
    assert_eq!(def.acceptance.runtime, RuntimeKind::Claude);
    assert!(
        def.workers
            .values()
            .all(|worker| worker.runtime == RuntimeKind::Claude)
    );
    Ok(())
}

#[test]
fn unknown_runtime_selectors_are_rejected() {
    let mut value = super::tests::complete();
    value["workers"]["db-dev"]["runtime"] = json!("other-provider");
    assert!(FleetDefinition::from_value(&value).is_err());
}

#[test]
fn codex_selector_reports_unsupported_runtime_without_claude_model_errors() -> anyhow::Result<()> {
    let mut value = super::tests::complete();
    value["workers"]["db-dev"]["runtime"] = json!("codex");
    value["workers"]["db-dev"]["model"] = json!("gpt-5");
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    let found = check_definition(&def);
    assert!(
        found
            .iter()
            .any(|item| item.item == "workers.db-dev.runtime")
    );
    assert!(!found.iter().any(|item| item.item == "workers.db-dev.model"));
    Ok(())
}

#[test]
fn runtime_selection_is_preserved_in_worker_template_and_reference_plans() -> anyhow::Result<()> {
    let mut value = super::tests::complete();
    value["workers"]["db-dev"]["runtime"] = json!("codex");
    value["reference"] = json!({"enabled": true});
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    let facts = super::plan_tests::facts();
    let plans = [
        plan::worker(&def, &facts, "P1", Uuid::nil()),
        plan::template(&def, &facts, "db-dev"),
        plan::reference(&def, &facts, "db-dev", Uuid::nil()),
    ];
    for candidate in plans {
        let seat = candidate.ok_or_else(|| anyhow::anyhow!("fixture should produce a seat"))?;
        assert_eq!(seat.runtime, RuntimeKind::Codex);
        assert!(seat.runtime.ensure_managed_launch_supported().is_err());
    }
    Ok(())
}

#[test]
fn runtime_changes_invalidate_brief_versions() -> anyhow::Result<()> {
    let mut def =
        FleetDefinition::from_value(&super::tests::complete()).map_err(anyhow::Error::msg)?;
    let before = brief_version::task_list_version(&def);
    def.coordinator.runtime = RuntimeKind::Codex;
    assert_ne!(before, brief_version::task_list_version(&def));
    def.coordinator.runtime = RuntimeKind::Claude;
    let worker = def
        .workers
        .get_mut("db-dev")
        .ok_or_else(|| anyhow::anyhow!("missing worker fixture"))?;
    worker.runtime = RuntimeKind::Codex;
    assert_ne!(before, brief_version::task_list_version(&def));
    Ok(())
}

#[test]
fn template_plans_preserve_runtime_and_legacy_serialized_plans_default_to_claude()
-> anyhow::Result<()> {
    let mut def =
        FleetDefinition::from_value(&super::tests::complete()).map_err(anyhow::Error::msg)?;
    let worker = def
        .workers
        .get_mut("db-dev")
        .ok_or_else(|| anyhow::anyhow!("missing worker fixture"))?;
    worker.runtime = RuntimeKind::Codex;
    let template = plan::template(&def, &super::plan_tests::facts(), "db-dev")
        .ok_or_else(|| anyhow::anyhow!("missing template"))?;
    assert_eq!(template.runtime, RuntimeKind::Codex);
    let mut value = serde_json::to_value(&template)?;
    value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("plan is not an object"))?
        .remove("runtime");
    let legacy: plan::SeatPlan = serde_json::from_value(value)?;
    assert_eq!(legacy.runtime, RuntimeKind::Claude);
    Ok(())
}
