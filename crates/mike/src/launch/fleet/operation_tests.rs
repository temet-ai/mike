use super::*;
use serde_json::json;

fn declared() -> anyhow::Result<FleetDefinition> {
    Ok(serde_json::from_value(
        json!({"execution": {"operations": {
            "web": {"command": "npm run typecheck", "purpose": "TypeScript types", "resources": ["web-cache"]},
            "python": {"command": "python3 -m unittest", "purpose": "Python unit tests", "resources": ["test-db"]},
            "docs": {"command": "scripts/check-links", "purpose": "document links", "resources": []}
        }}}),
    )?)
}

#[test]
fn operation_matching_is_exact_and_unknown_commands_have_no_reuse() -> anyhow::Result<()> {
    let mut def = declared()?;
    assert!(def.execution.operation("npm run typecheck").is_some());
    assert!(def.execution.operation("echo npm run typecheck").is_none());
    assert!(
        def.execution
            .operation("npm run typecheck && deploy")
            .is_none()
    );
    def.check.command = "python3 -m unittest".into();
    assert!(!reusable(&def));
    assert!(!def.execution.cargo);
    assert!(!context(&def).contains("Cargo"));
    assert!(validate(&def).is_empty());
    assert_eq!(adoption(&def)["check_declared"], true);
    assert_eq!(adoption(&def)["declared_operations"], 3);
    assert_eq!(adoption(&def)["cargo_adapter"], false);
    Ok(())
}

#[test]
fn invalid_operation_contracts_are_rejected() -> anyhow::Result<()> {
    let def: FleetDefinition = serde_json::from_value(json!({"execution": {"operations": {
        "a": {"command": "check", "purpose": "check", "cache_key": "", "timeout_seconds": 0,
            "resources": [""], "inputs": ["../secret"], "environment": {"MIKE_RUN_ID": "fake"}},
        "b": {"command": "check", "purpose": ""}
    }}}))?;
    let found = validate(&def);
    for text in [
        "cache_key",
        "timeout_seconds",
        "resource names",
        "repository-relative",
        "environment names",
        "already has",
        "purpose",
    ] {
        assert!(
            found.iter().any(|p| p.problem.contains(text)),
            "{text}: {found:?}"
        );
    }
    assert!(
        serde_json::from_value::<FleetDefinition>(json!({"execution": {"cargoo": true}})).is_err()
    );
    Ok(())
}

#[test]
fn gate_receipts_are_invalidated_by_environment_and_adapter_changes() -> anyhow::Result<()> {
    let mut def = declared()?;
    def.gate.command = "python3 -m unittest".into();
    let original = super::super::gate::ledger_for(&def, "run");
    def.execution
        .operations
        .get_mut("python")
        .ok_or_else(|| anyhow::anyhow!("python operation missing"))?
        .environment
        .insert("MODE".into(), "changed".into());
    let changed = super::super::gate::ledger_for(&def, "run");
    assert_ne!(original, changed);
    def.execution.cargo = true;
    assert_ne!(changed, super::super::gate::ledger_for(&def, "run"));
    Ok(())
}

#[tokio::test]
async fn cache_identity_covers_inputs_environment_and_toolchain() -> anyhow::Result<()> {
    let repo = tempfile::tempdir()?;
    let git = |args: &[&str]| -> anyhow::Result<()> {
        let result = std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(args)
            .output()?;
        anyhow::ensure!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(())
    };
    git(&["init", "-q"])?;
    git(&["config", "user.email", "test@example.com"])?;
    git(&["config", "user.name", "test"])?;
    std::fs::write(repo.path().join("input.py"), "assert 2 + 2 == 4\n")?;
    git(&["add", "input.py"])?;
    git(&["commit", "-qm", "initial"])?;
    let mut def: FleetDefinition = serde_json::from_value(json!({
        "repo": repo.path(), "check": {"command": "python3 input.py"},
        "execution": {"operations": {"test": {"command": "python3 input.py", "purpose": "assertion",
            "inputs": ["input.py"], "cache_key": "python-version-1", "environment": {"MODE": "test"}}}}
    }))?;
    let first = super::super::check_state::state_for(&def, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    std::fs::write(repo.path().join("README.md"), "unrelated\n")?;
    git(&["add", "README.md"])?;
    git(&["commit", "-qm", "docs"])?;
    let same = super::super::check_state::state_for(&def, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(first, same);
    def.execution
        .operations
        .get_mut("test")
        .ok_or_else(|| anyhow::anyhow!("test missing"))?
        .environment
        .insert("MODE".into(), "production".into());
    let env = super::super::check_state::state_for(&def, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(same, env);
    def.execution
        .operations
        .get_mut("test")
        .ok_or_else(|| anyhow::anyhow!("test missing"))?
        .cache_key = Some("python-version-2".into());
    let toolchain = super::super::check_state::state_for(&def, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(env, toolchain);
    std::fs::write(repo.path().join("input.py"), "assert False\n")?;
    git(&["add", "input.py"])?;
    git(&["commit", "-qm", "changed input"])?;
    let changed = super::super::check_state::state_for(&def, "HEAD")
        .await
        .map_err(anyhow::Error::msg)?;
    assert_ne!(toolchain, changed);
    Ok(())
}
