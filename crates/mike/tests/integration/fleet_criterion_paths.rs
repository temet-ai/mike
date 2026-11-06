use mike::launch::fleet::{FleetDefinition, check_definition, merge};
use serde_json::{Value, json};

const CALLER: &str = "crates/gg-sessions/src/review/save.rs";
const IMPLEMENTATION: &str = "crates/gg-workspace/src/okf.rs";

fn definition(patch: Value) -> anyhow::Result<FleetDefinition> {
    let mut value = json!({
        "goal": "Reviews are saved on their work items",
        "ask": "save the review on its work item",
        "repo": "/srv/project",
        "branch": "main",
        "criteria": {
            "C5": {"text": "The review is saved as a local note on the MR's work item",
                   "check": "Inspect the note on the work item",
                   "paths": [CALLER, IMPLEMENTATION]},
            "C6": {"text": "The review is shown", "check": "Inspect the review page"}
        },
        "coordinator": {"model": "opus", "agent": "fleet-coordinator", "brief": "Coordinate"},
        "acceptance": {"model": "opus", "agent": "fleet-acceptance", "brief": "Judge"},
        "observer": {"model": "sonnet", "agent": "fleet-observer", "brief": "Observe",
                     "contract": ".claude/fleet/observer-contract.md"},
        "workers": {"rust": {"model": "sonnet", "agent": "rust-developer"}},
        "tasks": {
            "T11": {"name": "Review journey", "discipline": "rust", "criteria": ["C5"],
                    "files": ["crates/gg-sessions/src/review/"], "brief": "Save the review"},
            "T12": {"name": "Review page", "discipline": "rust", "criteria": ["C6"],
                    "files": ["crates/gg-ui/src/review.rs"], "brief": "Show the review"}
        }
    });
    merge(&mut value, &patch);
    FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)
}

#[test]
fn criterion_path_crossing_crates_reports_the_unowned_final_step() -> anyhow::Result<()> {
    let def = definition(json!({}))?;
    assert!(
        def.tasks
            .values()
            .all(|task| task.required_paths.is_empty())
    );
    let missing = check_definition(&def);
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].item, "criteria.C5.paths.1");
    assert!(missing[0].problem.contains(IMPLEMENTATION));
    assert!(missing[0].problem.contains("no task owns"));
    assert!(missing[0].problem.contains("files or new_files"));
    Ok(())
}

#[test]
fn criterion_path_with_a_final_step_owner_has_nothing_missing() -> anyhow::Result<()> {
    let def = definition(json!({"tasks": {"T14": {
        "name": "Save the local note", "discipline": "rust", "criteria": ["C5"],
        "files": ["crates/gg-workspace/src/"], "brief": "Write the note on its work item"
    }}}))?;
    let missing = check_definition(&def);
    assert!(missing.is_empty(), "{missing:?}");
    Ok(())
}

#[test]
fn criterion_path_owned_as_a_new_file_is_covered() -> anyhow::Result<()> {
    let def = definition(json!({"tasks": {"T11": {"new_files": [IMPLEMENTATION]}}}))?;
    assert!(check_definition(&def).is_empty());
    Ok(())
}

#[test]
fn criterion_paths_preserve_existing_directory_ownership_patterns() -> anyhow::Result<()> {
    for scope in [
        "crates/gg-workspace/src",
        "crates/gg-workspace/src/",
        "crates/gg-workspace/src/*",
        "crates/gg-workspace/src/**",
    ] {
        let def = definition(json!({"tasks": {"T12": {"files": [scope]}}}))?;
        assert!(check_definition(&def).is_empty(), "{scope}");
    }
    Ok(())
}

#[test]
fn criterion_path_ownership_respects_directory_boundaries() -> anyhow::Result<()> {
    for scope in [
        "crates/gg-workspace/src/ok",
        "crates/gg-workspace/src/okf.rs/child",
        "crates/gg-workspace/src/other.rs",
    ] {
        let def = definition(json!({"tasks": {"T12": {"files": [scope]}}}))?;
        let missing = check_definition(&def);
        assert_eq!(missing.len(), 1, "{scope}: {missing:?}");
        assert!(missing[0].problem.contains("no task owns"));
    }
    Ok(())
}

#[test]
fn criterion_path_listed_as_shared_still_needs_a_task_owner() -> anyhow::Result<()> {
    let def = definition(json!({"shared_files": [IMPLEMENTATION]}))?;
    let missing = check_definition(&def);
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].item, "criteria.C5.paths.1");
    assert!(missing[0].problem.contains("no task owns"));
    Ok(())
}

#[test]
fn criterion_paths_reject_invalid_paths_even_if_a_task_lists_them() -> anyhow::Result<()> {
    for path in [
        "",
        ".",
        "..",
        "../okf.rs",
        "/src/okf.rs",
        "src/../okf.rs",
        "src//okf.rs",
        "src/*.rs",
        "src/ok?.rs",
        "src/[ok].rs",
        "src\\okf.rs",
        "`src/okf.rs`",
        " src/okf.rs",
        "src/okf.rs ",
    ] {
        let def = definition(json!({
            "criteria": {"C5": {"paths": [path]}},
            "tasks": {"T12": {"files": [path]}}
        }))?;
        let missing = check_definition(&def);
        assert!(
            missing.iter().any(|item| item.item == "criteria.C5.paths.0"
                && item.problem.contains("repository-relative")),
            "{path}: {missing:?}"
        );
    }
    Ok(())
}

#[test]
fn historical_criteria_omit_paths_without_inferring_them_from_prose() -> anyhow::Result<()> {
    let def = definition(json!({"criteria": {"C5": {
        "text": format!("The review is saved by {IMPLEMENTATION}"),
        "check": format!("Inspect {IMPLEMENTATION}"), "paths": null
    }}}))?;
    assert!(def.criteria["C5"].paths.is_empty());
    assert!(check_definition(&def).is_empty());
    let serialized = serde_json::to_value(&def)?;
    assert!(serialized["criteria"]["C5"].get("paths").is_none());
    Ok(())
}

#[test]
fn declared_criterion_paths_survive_serialization_and_replace_as_a_list() -> anyhow::Result<()> {
    let original = definition(json!({}))?;
    let mut value = serde_json::to_value(&original)?;
    assert_eq!(
        value["criteria"]["C5"]["paths"],
        json!([CALLER, IMPLEMENTATION])
    );
    merge(
        &mut value,
        &json!({"criteria": {"C5": {"paths": [CALLER]}}}),
    );
    let patched = FleetDefinition::from_value(&value).map_err(anyhow::Error::msg)?;
    assert_eq!(patched.criteria["C5"].paths, vec![CALLER]);
    assert_eq!(patched.criteria["C5"].text, original.criteria["C5"].text);
    assert!(check_definition(&patched).is_empty());
    Ok(())
}

#[test]
fn criterion_paths_reject_non_list_and_non_string_shapes() {
    for paths in [
        json!(IMPLEMENTATION),
        json!({"path": IMPLEMENTATION}),
        json!([7]),
    ] {
        assert!(definition(json!({"criteria": {"C5": {"paths": paths}}})).is_err());
    }
}
