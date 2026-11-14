use serde_json::{Value, json};

use super::definition_patch::{merged, parse_new};
use super::definition_view::{DefinitionError, seat_view};

fn one_owner() -> Value {
    json!({
        "shared_files": ["src/host.rs"],
        "tasks": {"T9": {"files": ["src/host.rs"]}}
    })
}

#[test]
fn define_patch_refuses_new_shared_overlap_without_a_producer_or_order() {
    let patch = json!({"tasks": {"T16": {"files": ["src/host.rs"]}}});
    let Err(DefinitionError::InvalidDefinition(problem)) = merged(&one_owner(), Some(&patch))
    else {
        panic!("new ambiguous sharing must fail before it is saved");
    };
    assert!(problem.contains("shared_file_producers"));
    assert!(problem.contains("src/host.rs"));
}

#[test]
fn define_patch_accepts_a_shared_producer_decided_with_the_new_task() -> anyhow::Result<()> {
    let patch = json!({
        "shared_file_producers": {"src/host.rs": "T9"},
        "tasks": {"T16": {"files": ["src/host.rs"]}}
    });
    let (_, def) = merged(&one_owner(), Some(&patch))?;
    assert_eq!(def.shared_file_producers["src/host.rs"], "T9");
    Ok(())
}

#[test]
fn define_patch_accepts_an_explicit_valid_shared_order() -> anyhow::Result<()> {
    let patch = json!({"tasks": {"T16": {"files": ["src/host.rs"], "after": ["T9"]}}});
    assert!(merged(&one_owner(), Some(&patch)).is_ok());
    Ok(())
}

#[test]
fn define_patch_preserves_historical_overlap_on_unrelated_edits() -> anyhow::Result<()> {
    let mut stored = one_owner();
    stored["tasks"]["T16"] = json!({"files": ["src/host.rs"]});
    let (_, def) = merged(&stored, Some(&json!({"goal": "keep working"})))?;
    assert_eq!(def.goal, "keep working");
    Ok(())
}

#[test]
fn create_definition_enforces_overlap_decision_from_the_start() {
    let mut definition = one_owner();
    definition["tasks"]["T16"] = json!({"files": ["src/host.rs"]});
    assert!(matches!(
        parse_new(Some(definition)),
        Err(DefinitionError::InvalidDefinition(_))
    ));
}

#[test]
fn seat_view_exposes_the_required_path_and_shared_producer_contract() -> anyhow::Result<()> {
    let patch = json!({
        "shared_file_producers": {"src/host.rs": "T9"},
        "tasks": {"T6": {"files": ["src/menu.rs"],
            "required_paths": {"src/host.rs": "T9"}}}
    });
    let (_, def) = merged(&one_owner(), Some(&patch))?;
    let session = uuid::Uuid::from_u128(6);
    let sessions = std::collections::BTreeMap::from([("T6".to_string(), session)]);
    let view = seat_view(&def, Some(&session.to_string()), &sessions);
    assert_eq!(view["you"]["required_paths"]["src/host.rs"], "T9");
    assert_eq!(view["shared_file_producers"]["src/host.rs"], "T9");
    Ok(())
}
