use mike::launch::fleet::{FleetDefinition, check_definition};
use serde_json::json;

fn definition() -> anyhow::Result<FleetDefinition> {
    FleetDefinition::from_value(&json!({
        "criteria": {"C1": {"text": "Protocol fields render with stable module parts",
            "paths": ["src/protocol.rs", "tests/parts.rs", "tests/ink.test.ts"]}},
        "tasks": {"T2": {"criteria": ["C1"], "files": ["src/protocol.rs"]}}
    }))
    .map_err(anyhow::Error::msg)
}

#[test]
fn declared_cross_cutting_tests_need_owners_beyond_protocol_implementation() -> anyhow::Result<()> {
    let missing = check_definition(&definition()?);
    let paths: Vec<_> = missing
        .iter()
        .filter(|item| item.item.starts_with("criteria.C1.paths"))
        .collect();
    assert_eq!(paths.len(), 2, "{missing:?}");
    assert!(
        paths
            .iter()
            .any(|item| item.problem.contains("tests/parts.rs"))
    );
    assert!(
        paths
            .iter()
            .any(|item| item.problem.contains("tests/ink.test.ts"))
    );
    assert!(
        paths
            .iter()
            .all(|item| item.problem.contains("no task owns"))
    );
    Ok(())
}

#[test]
fn assigning_cross_cutting_test_paths_closes_only_the_declared_ownership_gap() -> anyhow::Result<()>
{
    let mut def = definition()?;
    def.tasks.entry("parts".into()).or_default().files = vec!["tests/parts.rs".into()];
    def.tasks.entry("ink".into()).or_default().files = vec!["tests/ink.test.ts".into()];
    let missing = check_definition(&def);
    assert!(
        !missing
            .iter()
            .any(|item| item.item.starts_with("criteria.C1.paths")),
        "{missing:?}"
    );
    assert!(def.tasks.values().all(|task| task.session.is_none()));
    Ok(())
}
