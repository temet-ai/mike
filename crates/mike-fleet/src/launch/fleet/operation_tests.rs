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
    assert!(!context(&def).contains("times your own runs"));
    assert!(validate(&def).is_empty());
    assert_eq!(adoption(&def)["check_declared"], true);
    assert_eq!(adoption(&def)["declared_operations"], 3);
    assert_eq!(adoption(&def)["timed_commands"], 0);
    Ok(())
}

#[test]
fn a_stored_definition_with_retired_settings_parses_and_ignores_them() -> anyhow::Result<()> {
    let stored = json!({"build_slots": 3, "execution": {"cargo": true, "operations": {}}});
    let def = FleetDefinition::from_value(&stored).map_err(anyhow::Error::msg)?;
    assert!(def.execution.timed_commands.is_empty());
    assert!(def.execution.disabled_plugins.is_empty());
    assert!(def.execution.strip_environment.is_empty());
    assert!(validate(&def).is_empty());
    assert_eq!(unread_warnings(&def).len(), 1);
    assert_eq!(unread_warnings(&def)[0].item, "execution.cargo");
    Ok(())
}

#[test]
fn a_typo_in_an_execution_setting_warns_and_does_not_refuse() -> anyhow::Result<()> {
    let def: FleetDefinition =
        serde_json::from_value(json!({"execution": {"timed_comands": {"make": {}}}}))?;
    assert!(def.execution.timed_commands.is_empty());
    assert!(unread_warnings(&def)[0].problem.contains("`timed_comands`"));
    Ok(())
}

#[test]
fn a_timed_program_is_a_bare_name_and_agrees_on_shared_capacity() -> anyhow::Result<()> {
    let def: FleetDefinition = serde_json::from_value(json!({"execution": {
        "operations": {"a": {"command": "make all", "purpose": "builds",
            "resources": [{"name": "build", "capacity": 2}]}},
        "timed_commands": {
            "make": {"resources": [{"name": "build", "capacity": 3}]},
            "../evil": {},
            "with space": {}
        },
        "strip_environment": ["BUILD_DIR", "MIKE_RUN_ID", "NOT VALID"],
        "package_manifests": [{"file": "../manifest", "label": "unit"}]
    }}))?;
    let found = validate(&def);
    for (item, text) in [
        ("execution.timed_commands.../evil", "`../evil`"),
        ("execution.timed_commands.with space", "`with space`"),
        ("execution.timed_commands.make", "same capacity"),
        ("execution.strip_environment", "`MIKE_RUN_ID`"),
        ("execution.strip_environment", "`NOT VALID`"),
        ("execution.package_manifests", "`../manifest`"),
    ] {
        assert!(
            found
                .iter()
                .any(|p| p.item == item && p.problem.contains(text)),
            "{item} {text}: {found:?}"
        );
    }
    assert!(!found.iter().any(|p| p.problem.contains("BUILD_DIR")));
    Ok(())
}

#[test]
fn a_seat_is_told_what_the_project_times_and_backgrounds() -> anyhow::Result<()> {
    let def: FleetDefinition = serde_json::from_value(json!({"execution": {
        "timed_commands": {"make": {}},
        "background_commands": ["make all"]
    }}))?;
    let told = context(&def);
    assert!(told.contains("times your own runs of make"), "{told}");
    assert!(told.contains("make all"), "{told}");
    assert_eq!(adoption(&def)["timed_commands"], 1);
    Ok(())
}

#[test]
fn a_timed_command_covers_direct_and_wrapped_runs_of_its_subcommands() {
    let timed = TimedCommand {
        subcommands: vec!["check".into(), "test".into()],
        through: vec!["scripts/cx".into()],
        ..TimedCommand::default()
    };
    for command in [
        "cargo check -p a",
        "cd crates && cargo test 2>&1 | tail",
        "scripts/cx check -p a",
        "FOO=1 ./scripts/cx test",
    ] {
        assert!(timed.covers("cargo", command, &[]), "{command}");
    }
    for command in ["cargo fmt", "scripts/cx fmt", "make test", "git status"] {
        assert!(!timed.covers("cargo", command, &[]), "{command}");
    }
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
    Ok(())
}

#[test]
fn gate_receipts_are_invalidated_by_environment_and_strip_changes() -> anyhow::Result<()> {
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
    def.execution.strip_environment = vec!["BUILD_DIR".into()];
    assert_ne!(changed, super::super::gate::ledger_for(&def, "run"));
    Ok(())
}

#[test]
fn a_project_with_nothing_to_strip_keeps_the_ledger_it_had_before() -> anyhow::Result<()> {
    let mut def = declared()?;
    def.gate.command = "python3 -m unittest".into();
    let operation = def.execution.operation(&def.gate.command);
    let policy = json!([false, operation]).to_string();
    let identity = super::super::check_state::command_state(&policy, &def.gate.command);
    assert_eq!(
        super::super::gate::ledger_for(&def, "run"),
        format!("{}:{identity}", super::super::gate::ledger("run"))
    );
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
