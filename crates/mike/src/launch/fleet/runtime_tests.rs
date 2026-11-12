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
fn codex_worker_is_checked_by_codex_rules_not_claude_rules() -> anyhow::Result<()> {
    let mut value = super::tests::complete();
    value["workers"]["db-dev"]["runtime"] = json!("codex");
    value["workers"]["db-dev"]["model"] = json!("gpt-5");
    value["workers"]["db-dev"]["agent"] = json!("");
    value["workers"]["db-dev"]["effort"] = json!("minimal");
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    let found = check_definition(&def);
    assert!(
        !found
            .iter()
            .any(|item| item.item.starts_with("workers.db-dev")),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn codex_worker_with_claude_model_agent_or_effort_is_reported() -> anyhow::Result<()> {
    let mut value = super::tests::complete();
    value["workers"]["db-dev"]["runtime"] = json!("codex");
    value["workers"]["db-dev"]["model"] = json!("sonnet");
    value["workers"]["db-dev"]["effort"] = json!("max");
    let def = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    let found = check_definition(&def);
    for item in [
        "workers.db-dev.model",
        "workers.db-dev.agent",
        "workers.db-dev.effort",
    ] {
        assert!(found.iter().any(|f| f.item == item), "{item} in {found:?}");
    }
    assert!(!found.iter().any(|f| f.item == "workers.db-dev.runtime"));
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
        assert!(!seat.runtime.controls_through_terminal());
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
