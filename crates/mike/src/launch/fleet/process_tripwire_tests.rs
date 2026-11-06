use super::*;

fn definition() -> FleetDefinition {
    FleetDefinition {
        repo: "/tmp/project".into(),
        check: super::super::definition::CheckDefinition {
            command: "make check".into(),
            ..Default::default()
        },
        gate: super::super::definition::GateDefinition {
            command: "make full".into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn unchanged_command_in_managed_checkout_does_not_warn() {
    let run = Uuid::new_v4().to_string();
    let def = definition();
    let _owned = register(
        &run,
        Path::new("/tmp/project/.claude/worktrees/mike-gate"),
        &def.gate.command,
        Some(Setting::Gate),
        42,
    );
    assert!(notices(&run, &def, Uuid::new_v4()).is_empty());
}

#[test]
fn changed_setting_warns_only_while_owned_execution_is_alive() {
    let run = Uuid::new_v4().to_string();
    let coordinator = Uuid::new_v4();
    let mut def = definition();
    let owned = register(
        &run,
        Path::new(&def.repo),
        &def.gate.command,
        Some(Setting::Gate),
        42,
    );
    def.gate.command = "make quick".into();
    let warnings = notices(&run, &def, coordinator);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].to, coordinator);
    assert!(warnings[0].key.starts_with("process-setting:"));
    assert!(warnings[0].body.contains("make full"));
    assert!(warnings[0].body.contains("make quick"));
    assert!(warnings[0].body.contains("no hang or failure is inferred"));
    assert!(notices("another-run", &def, coordinator).is_empty());
    assert_eq!(notices(&run, &def, coordinator)[0].key, warnings[0].key);
    drop(owned);
    assert!(notices(&run, &def, coordinator).is_empty());
}

#[test]
fn check_and_gate_settings_are_compared_independently() {
    let run = Uuid::new_v4().to_string();
    let mut def = definition();
    let _owned = register(
        &run,
        Path::new(&def.repo),
        &def.check.command,
        Some(Setting::Check),
        43,
    );
    def.gate.command.clear();
    assert!(notices(&run, &def, Uuid::new_v4()).is_empty());
    def.check.command.clear();
    assert_eq!(notices(&run, &def, Uuid::new_v4()).len(), 1);
    def.check.command = "  make check  ".into();
    assert!(notices(&run, &def, Uuid::new_v4()).is_empty());
}

#[test]
fn explicit_command_has_no_declared_setting_to_outlive() {
    let run = Uuid::new_v4().to_string();
    let def = definition();
    assert!(register(&run, Path::new(&def.repo), "make other", None, 44).is_none());
    assert!(notices(&run, &def, Uuid::new_v4()).is_empty());
}
