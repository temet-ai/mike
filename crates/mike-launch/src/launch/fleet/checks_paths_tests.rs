use serde_json::json;

use super::{brief, changed_overlaps, producer_for, validate};
use crate::launch::fleet::checks::check_definition;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::propose::{Judged, judge};
use crate::launch::fleet::propose_tests::proposal;
use crate::launch::fleet::tests::changed;

const SHARED: &str = "crates/mike/src/store/persistence/mod.rs";

fn shared_overlap() -> FleetDefinition {
    changed(json!({"tasks": {"P2": {"files": [SHARED]}}}))
}

#[test]
fn declarations_absent_in_historical_definitions_default_empty() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&json!({"tasks": {"T1": {"files": ["src/a"]}}}))
        .map_err(anyhow::Error::msg)?;
    assert!(def.shared_file_producers.is_empty());
    assert!(def.tasks["T1"].required_paths.is_empty());
    assert!(validate(&def).is_empty());
    Ok(())
}

#[test]
fn required_path_outside_named_producers_files_is_reported() {
    let def = changed(json!({"tasks": {"P2": {"required_paths": {
        "crates/gg-core/src/host.rs": "P1"
    }}}}));
    let found = check_definition(&def);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .item
            .contains("required_paths.crates/gg-core/src/host.rs")
    );
    assert!(found[0].problem.contains("P1 does not own"));
}

#[test]
fn required_path_covered_by_a_directory_or_new_file_passes() {
    let def = changed(json!({"tasks": {
        "P1": {"new_files": ["src/host.rs"]},
        "P2": {"required_paths": {
            "crates/mike/src/store/persistence/improvement/rows.rs": "P1",
            "src/host.rs": "P1"
        }}
    }}));
    assert!(validate(&def).is_empty());
}

#[test]
fn required_path_names_an_existing_other_task() {
    for producer in ["P2", "missing", ""] {
        let def = changed(json!({"tasks": {"P2": {"required_paths": {
            SHARED: producer
        }}}}));
        assert!(!validate(&def).is_empty(), "{producer}");
    }
}

#[test]
fn required_paths_reject_traversal_absolute_empty_and_glob_paths() {
    for path in [
        "",
        "../host.rs",
        "/src/host.rs",
        "src/*",
        "src/../host.rs",
        "src//host.rs",
    ] {
        let def = changed(json!({"tasks": {"P2": {"required_paths": {path: "P1"}}}}));
        assert!(
            validate(&def)
                .iter()
                .any(|m| m.problem.contains("repository-relative")),
            "{path}"
        );
    }
}

#[test]
fn producer_coverage_respects_directory_boundaries() {
    let def = changed(json!({"tasks": {
        "P1": {"files": ["src/host"]},
        "P2": {"required_paths": {"src/hosting.rs": "P1"}}
    }}));
    assert!(
        validate(&def)
            .iter()
            .any(|m| m.problem.contains("does not own"))
    );
}

#[test]
fn shared_producer_must_own_a_declared_shared_path() {
    for patch in [
        json!({"shared_file_producers": {SHARED: "P2"}}),
        json!({"shared_file_producers": {SHARED: "missing"}}),
        json!({"shared_file_producers": {"docs/IMPROVEMENT_LOOP.md": "P10"}}),
    ] {
        assert!(!validate(&changed(patch)).is_empty());
    }
}

#[test]
fn conflicting_shared_producers_are_reported_and_do_not_resolve_overlap() {
    let def = changed(json!({
        "shared_files": ["src/"],
        "shared_file_producers": {"src": "P1", "src/host.rs": "P2"},
        "tasks": {"P1": {"files": ["src/"]}, "P2": {"files": ["src/"]}}
    }));
    assert!(
        validate(&def)
            .iter()
            .any(|m| m.problem.contains("conflicts"))
    );
    assert_eq!(producer_for(&def, "src"), None);
    assert_eq!(producer_for(&def, "src/host.rs"), None);
    assert!(!changed_overlaps(&FleetDefinition::default(), &def).is_empty());
}

#[test]
fn required_paths_cannot_name_a_different_shared_producer() {
    let def = changed(json!({
        "shared_file_producers": {SHARED: "P1"},
        "tasks": {"P2": {"files": [SHARED]}, "P10": {"required_paths": {SHARED: "P2"}}}
    }));
    assert!(
        validate(&def)
            .iter()
            .any(|m| m.problem.contains("shared producer P1"))
    );
}

#[test]
fn newly_shared_overlap_requires_a_producer_or_order() {
    let found = changed_overlaps(&changed(json!({})), &shared_overlap());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].problem.contains("shared_file_producers"));
}

#[test]
fn complete_new_definition_enforces_shared_overlaps() {
    assert_eq!(
        changed_overlaps(&FleetDefinition::default(), &shared_overlap()).len(),
        1
    );
}

#[test]
fn historical_overlap_unchanged_by_a_patch_remains_compatible() {
    let before = shared_overlap();
    let mut after = before.clone();
    after.goal = "updated goal".into();
    assert!(changed_overlaps(&before, &after).is_empty());
}

#[test]
fn shared_overlap_with_a_valid_producer_is_resolved() {
    let mut after = shared_overlap();
    after
        .shared_file_producers
        .insert(SHARED.into(), "P1".into());
    assert!(validate(&after).is_empty());
    assert!(changed_overlaps(&FleetDefinition::default(), &after).is_empty());
}

#[test]
fn shared_overlap_with_an_acyclic_order_is_resolved() {
    let after = changed(json!({"tasks": {"P2": {"files": [SHARED], "after": ["P1"]}}}));
    assert!(changed_overlaps(&FleetDefinition::default(), &after).is_empty());
}

#[test]
fn shared_overlap_with_a_cycle_or_missing_dependency_is_unresolved() {
    for after in [vec!["P10"], vec!["P1", "missing"]] {
        let def = changed(json!({"tasks": {"P2": {"files": [SHARED], "after": after}}}));
        assert_eq!(changed_overlaps(&FleetDefinition::default(), &def).len(), 1);
    }
}

#[test]
fn ordered_overlap_still_needs_a_shared_file_declaration() {
    let def = changed(json!({"tasks": {"P10": {
        "files": ["frontend/src/routes/improvement/page.svelte"], "after": ["P2"]
    }}}));
    assert_eq!(changed_overlaps(&FleetDefinition::default(), &def).len(), 1);
}

#[test]
fn removing_an_explicit_resolution_cannot_restore_an_ambiguous_overlap() {
    let mut before = shared_overlap();
    before
        .shared_file_producers
        .insert(SHARED.into(), "P1".into());
    assert_eq!(changed_overlaps(&before, &shared_overlap()).len(), 1);
}

#[test]
fn completed_task_does_not_hold_new_ownership() {
    let def = changed(json!({"tasks": {
        "P2": {"files": [SHARED]}
    }}));
    let done = std::collections::BTreeSet::from(["P1".to_string()]);
    assert!(super::changed_overlaps_done(&FleetDefinition::default(), &def, &done).is_empty());
}

#[test]
fn new_files_participate_in_overlap_decisions() {
    let def = changed(json!({"tasks": {
        "P1": {"new_files": ["src/new.rs"]}, "P2": {"new_files": ["src/new.rs"]}
    }}));
    let found = changed_overlaps(&FleetDefinition::default(), &def);
    assert_eq!(found.len(), 1);
    assert!(found[0].problem.contains("src/new.rs"));
}

#[test]
fn proposal_with_shared_files_is_held_until_producer_or_order_exists() {
    let def = changed(json!({}));
    let mut proposed = proposal(&[SHARED], &[]);
    assert!(matches!(
        judge(&def, &proposed, &|_| true),
        Judged::Overlap(_)
    ));
    proposed.after = vec!["P1".into()];
    assert_eq!(judge(&def, &proposed, &|_| true), Judged::Clean);
    proposed.after.clear();
    let assigned = changed(json!({"shared_file_producers": {SHARED: "P1"}}));
    assert_eq!(judge(&assigned, &proposed, &|_| true), Judged::Clean);
}

#[test]
fn proposal_cannot_use_a_cyclic_dependency_to_resolve_sharing() {
    let def = changed(json!({"tasks": {"P1": {"after": ["P1"]}}}));
    let mut proposed = proposal(&[SHARED], &[]);
    proposed.after = vec!["P1".into()];
    let Judged::Refused(found) = judge(&def, &proposed, &|_| true) else {
        panic!("the cyclic proposal must be refused");
    };
    assert!(found.iter().any(|m| m.contains("cyclic")));
}

#[test]
fn brief_includes_the_named_producer_and_peer_required_paths() {
    let def = changed(json!({
        "shared_file_producers": {SHARED: "P1"},
        "tasks": {"P2": {"required_paths": {SHARED: "P1"}}}
    }));
    let text = brief(&def, &def.tasks["P2"]);
    assert!(text.contains("task P1 writes the shared hunk"));
    assert!(text.contains("task P1 is the producer to ask"));
}

#[test]
fn named_shared_producer_takes_precedence_for_owner_lookup() {
    let def = changed(json!({
        "shared_file_producers": {SHARED: "P2"},
        "tasks": {"P2": {"files": [SHARED]}}
    }));
    let owner = crate::launch::fleet::commit_paths::owner_of(&def, SHARED);
    assert_eq!(
        owner.map(|(id, name)| (id.as_str(), name)),
        Some(("P2", "page"))
    );
}

#[test]
fn invalid_or_absent_shared_producer_preserves_legacy_owner_lookup() {
    for producer in [None, Some("missing")] {
        let mut def = shared_overlap();
        if let Some(producer) = producer {
            def.shared_file_producers
                .insert(SHARED.into(), producer.into());
        }
        let owner = crate::launch::fleet::commit_paths::owner_of(&def, SHARED);
        assert_eq!(owner.map(|(id, _)| id.as_str()), Some("P1"));
    }
}

fn with_operations(patch: serde_json::Value) -> FleetDefinition {
    let mut def = changed(patch);
    def.execution.operations.insert(
        "unit".into(),
        crate::launch::fleet::operation::Operation::default(),
    );
    def
}

#[test]
fn done_when_naming_an_undeclared_operation_is_refused() {
    let def = with_operations(json!({"tasks": {"P2": {"done_when": {
        "checks": [{"operation": "unit", "expected_exit": 0}, {"operation": "missing"}]
    }}}}));
    let found = validate(&def);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].item, "tasks.P2.done_when.checks.1.operation");
    assert!(
        found[0]
            .problem
            .contains("`missing` is not a declared operation")
    );
    assert!(found[0].problem.contains("unit"));
}

#[test]
fn done_when_exit_code_paths_and_exercise_are_validated() {
    let def = with_operations(json!({"tasks": {"P2": {"done_when": {
        "checks": [{"operation": "unit", "expected_exit": 126}],
        "tests_added_under": ["frontend/src/routes/improvement/"],
        "files_changed": ["docs/elsewhere.md", "../up"],
        "exercise": {"command": " ", "expect_output": "drawn"}
    }}}}));
    let items: Vec<String> = validate(&def).into_iter().map(|m| m.item).collect();
    assert_eq!(
        items,
        vec![
            "tasks.P2.done_when.checks.0.expected_exit",
            "tasks.P2.done_when.files_changed.0",
            "tasks.P2.done_when.files_changed.1",
            "tasks.P2.done_when.exercise.command",
        ]
    );
}

#[test]
fn a_complete_done_when_and_owned_exports_pass() {
    let def = with_operations(json!({"tasks": {"P2": {
        "exports": ["frontend/src/routes/improvement/page.ts"],
        "done_when": {
            "checks": [{"operation": "unit", "expected_exit": 0}],
            "files_changed": ["frontend/src/routes/improvement/page.ts"],
            "exercise": {"command": "scripts/run", "expect_output": "ok"}
        }
    }}}));
    assert!(validate(&def).is_empty(), "{:?}", validate(&def));
}

#[test]
fn an_empty_done_when_and_foreign_exports_are_reported() {
    let def = changed(json!({"tasks": {"P2": {
        "done_when": {},
        "exports": ["docs/IMPROVEMENT_LOOP.md", "src/*.rs"]
    }}}));
    let items: Vec<String> = validate(&def).into_iter().map(|m| m.item).collect();
    assert_eq!(
        items,
        vec![
            "tasks.P2.exports.0",
            "tasks.P2.exports.1",
            "tasks.P2.done_when"
        ]
    );
}

#[test]
fn done_when_rejects_unknown_fields() {
    let value = json!({"tasks": {"P2": {"done_when": {"check": []}}}});
    assert!(FleetDefinition::from_value(&value).is_err());
}

fn crate_manifest(root: &std::path::Path, dir: &str) -> anyhow::Result<()> {
    let path = root.join(dir);
    std::fs::create_dir_all(&path)?;
    std::fs::write(path.join("Cargo.toml"), "[package]\nname = \"x\"\n")?;
    Ok(())
}

#[test]
fn two_tasks_in_one_declared_package_get_a_warning() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    crate_manifest(root.path(), "crates/a")?;
    crate_manifest(root.path(), "crates/b")?;
    let def = changed(json!({
        "repo": root.path().to_string_lossy(),
        "execution": {"package_manifests": [
            {"file": "Cargo.toml", "contains": "[package]", "label": "Cargo package"}
        ]},
        "tasks": {
            "P1": {"files": ["crates/a/src/one.rs"]},
            "P2": {"files": ["crates/a/src/two/"], "new_files": ["crates/b/src/new.rs"]},
            "P10": {"files": ["docs/IMPROVEMENT_LOOP.md"]}
        }
    }));
    let found = super::compile_unit_warnings(&def);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].problem.contains("tasks P1, P2"));
    assert!(found[0].problem.contains("Cargo package `crates/a`"));
    Ok(())
}

#[test]
fn a_manifest_no_project_declared_makes_no_unit() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    crate_manifest(root.path(), "crates/a")?;
    let def = changed(json!({
        "repo": root.path().to_string_lossy(),
        "tasks": {
            "P1": {"files": ["crates/a/src/one.rs"]},
            "P2": {"files": ["crates/a/src/two.rs"]}
        }
    }));
    assert!(super::compile_unit_warnings(&def).is_empty());
    Ok(())
}

#[test]
fn a_workspace_root_package_json_is_not_one_compile_unit() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("package.json"),
        "{\"workspaces\": [\"web\"]}",
    )?;
    std::fs::create_dir_all(root.path().join("web/a"))?;
    std::fs::write(root.path().join("web/a/package.json"), "{}")?;
    let def = changed(json!({
        "repo": root.path().to_string_lossy(),
        "tasks": {
            "P1": {"files": ["web/a/x.ts"]},
            "P2": {"files": ["web/a/y.ts"]},
            "P10": {"files": ["README.md"]}
        }
    }));
    let found = super::compile_unit_warnings(&def);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .problem
            .contains("package.json workspace member `web/a`")
    );
    Ok(())
}
