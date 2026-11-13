use super::*;

const PLUGIN: &str = "language-server@plugins";

fn declared_fleet() -> FleetDefinition {
    FleetDefinition {
        execution: Execution {
            timed_commands: [(
                "make".to_string(),
                super::super::operation::TimedCommand::default(),
            )]
            .into_iter()
            .collect(),
            background_commands: vec!["make all".into()],
            disabled_plugins: vec![PLUGIN.into()],
            ..Execution::default()
        },
        ..Default::default()
    }
}

#[test]
fn a_fleet_that_declares_nothing_gets_no_build_settings() {
    let settings = effective(&FleetDefinition::default(), &scratch(), "worker");
    assert!(settings["env"].get(background_hook::COMMANDS_ENV).is_none());
    assert!(settings["env"].get(timed_command::SHIMS_ENV).is_none());
    assert!(settings["env"].get(timed_command::DECLARED_ENV).is_none());
    assert!(settings.get("enabledPlugins").is_none());
    assert!(!settings["hooks"].to_string().contains("background-hook"));
}

#[test]
fn a_stored_definition_with_the_retired_setting_gets_no_build_settings() -> anyhow::Result<()> {
    let def = FleetDefinition::from_value(&json!({"execution": {"cargo": true}}))
        .map_err(anyhow::Error::msg)?;
    let settings = effective(&def, &scratch(), "worker");
    assert!(settings["env"].get(timed_command::SHIMS_ENV).is_none());
    assert!(settings.get("enabledPlugins").is_none());
    Ok(())
}

#[test]
fn the_seat_environment_carries_the_declarations_its_helpers_read() -> anyhow::Result<()> {
    let settings = effective(&declared_fleet(), &scratch(), "worker-1");
    let timed = settings["env"][timed_command::DECLARED_ENV]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no declaration in {settings}"))?;
    assert!(timed_command::declared_in(timed, std::ffi::OsStr::new("make")).is_some());
    assert_eq!(
        settings["env"][background_hook::COMMANDS_ENV],
        json!(["make all"]).to_string()
    );
    Ok(())
}

fn scratch() -> PathBuf {
    scratch_dir(Path::new("/state/mike/fleet"), "/work/holdfast")
}

#[test]
fn scratch_sits_beside_the_fleet_state_and_is_named_for_the_repository() {
    assert_eq!(scratch(), PathBuf::from("/state/mike/seats/holdfast"));
    assert_eq!(
        scratch_dir(Path::new("/state/mike/fleet"), "/work/my repo"),
        PathBuf::from("/state/mike/seats/my_repo")
    );
}

#[test]
fn every_seat_gets_a_scratch_folder_and_config_home_outside_tmp_and_the_operators_home() {
    let settings = effective(&FleetDefinition::default(), &scratch(), "worker-1");
    let env = &settings["env"];
    assert_eq!(env["TMPDIR"], "/state/mike/seats/holdfast/tmp");
    assert_eq!(env["CLAUDE_CODE_TMPDIR"], env["TMPDIR"]);
    assert_eq!(env["XDG_CONFIG_HOME"], "/state/mike/seats/holdfast/config");
}

#[test]
fn a_declared_plugin_is_off_for_every_seat() {
    let settings = effective(&declared_fleet(), &scratch(), "worker-1");
    assert_eq!(settings["enabledPlugins"][PLUGIN], json!(false));
}

#[test]
fn the_definitions_seat_settings_win_over_the_defaults() {
    let def = FleetDefinition {
        seat_settings: Some(json!({
            "enabledPlugins": {PLUGIN: true},
            "env": {"TMPDIR": "/mine", "EXTRA": "1"},
        })),
        ..declared_fleet()
    };
    let settings = effective(&def, &scratch(), "worker-1");
    assert_eq!(settings["enabledPlugins"][PLUGIN], json!(true));
    assert_eq!(settings["env"]["TMPDIR"], "/mine");
    assert_eq!(settings["env"]["EXTRA"], "1");
    assert_eq!(
        settings["env"]["XDG_CONFIG_HOME"],
        "/state/mike/seats/holdfast/config"
    );
}

#[test]
fn a_seat_gets_the_tool_profile_of_its_kind() {
    for kind in ["coordinator", "acceptance", "observer", "reference"] {
        let settings = effective(&FleetDefinition::default(), &scratch(), kind);
        assert_eq!(settings["env"][PROFILE_ENV], kind);
    }
    for seat in ["rust-developer-T8", "template-rust-developer", "db-dev-T5"] {
        let settings = effective(&FleetDefinition::default(), &scratch(), seat);
        assert_eq!(settings["env"][PROFILE_ENV], "worker", "{seat}");
    }
}

#[test]
fn the_definitions_seat_settings_can_name_another_profile() {
    let def = FleetDefinition {
        seat_settings: Some(json!({"env": {PROFILE_ENV: "full"}})),
        ..FleetDefinition::default()
    };
    let settings = effective(&def, &scratch(), "observer");
    assert_eq!(settings["env"][PROFILE_ENV], "full");
}
#[test]
fn numbered_control_successors_keep_their_role_profile() {
    for role in KINDS_WITH_OWN_PROFILE {
        assert_eq!(profile_for(&format!("{role}-2")), role);
    }
    for seat in ["acceptance-other", "acceptance-", "acceptance-2-T3"] {
        assert_eq!(profile_for(seat), "worker");
    }
}
#[test]
fn fleet_defaults_keep_operator_guards_and_add_the_background_hook() {
    let operator = json!({"matcher": "Edit", "hooks": [{"type": "command", "command": "guard"}]});
    let def = FleetDefinition {
        seat_settings: Some(json!({"hooks": {"PreToolUse": [operator.clone()]}})),
        ..declared_fleet()
    };
    let settings = effective(&def, &scratch(), "worker-1");
    assert_eq!(settings["hooks"]["PreToolUse"][0], operator);
    assert_eq!(settings["hooks"]["PreToolUse"][1]["matcher"], "Bash");
    let disabled = FleetDefinition {
        seat_settings: Some(json!({"env": {background_hook::COMMANDS_ENV: "[]"}})),
        ..declared_fleet()
    };
    let hooks = effective(&disabled, &scratch(), "worker-1")["hooks"]["PreToolUse"].clone();
    let commands: Vec<String> = hooks
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e["hooks"][0]["command"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(commands.len(), 1, "{hooks}");
    assert!(commands[0].ends_with("fleet receipt-hook"), "{hooks}");
    let silent = FleetDefinition {
        seat_settings: Some(json!({"env": {
            background_hook::COMMANDS_ENV: "[]",
            receipt_hook::ENABLED_ENV: "0",
        }})),
        ..declared_fleet()
    };
    assert!(
        effective(&silent, &scratch(), "worker-1")["hooks"]
            .get("PreToolUse")
            .is_none()
    );
}
#[test]
fn fleet_roles_and_successors_suppress_browser_helpers_by_default() {
    for seat in [
        "worker-T1",
        "coordinator",
        "acceptance",
        "acceptance-2",
        "observer",
        "reference",
        "template-rust",
    ] {
        let settings = effective(&FleetDefinition::default(), &scratch(), seat);
        assert_eq!(settings["env"]["BROWSER"], "true", "{seat}");
        let start = settings["hooks"]["SessionStart"].to_string();
        assert!(
            !start.contains("/state/mike/seats/holdfast/bin/activate"),
            "{seat}: {start}"
        );
    }
}

#[test]
fn seats_share_no_machine_wide_build_queue_and_get_no_build_settings_mike_chose() {
    let settings = effective(&declared_fleet(), &scratch(), "worker-T1");
    assert_eq!(
        settings["env"][timed_command::SHIMS_ENV],
        "/state/mike/seats/holdfast/bin"
    );
    let env = settings["env"].as_object().cloned().unwrap_or_default();
    for name in [
        "MIKE_FLEET_BUILD_SLOTS",
        "MIKE_FLEET_BUILD_SLOT_DIR",
        "BUILD_JOBS",
    ] {
        assert!(!env.contains_key(name), "{name}");
    }
    let def = FleetDefinition {
        seat_settings: Some(json!({"env": {"BUILD_JOBS": "6"}})),
        ..declared_fleet()
    };
    let settings = effective(&def, &scratch(), "worker-T1");
    assert_eq!(settings["env"]["BUILD_JOBS"], "6");
}

#[test]
fn a_timed_shim_never_names_a_deleted_binary() -> std::io::Result<()> {
    let scratch = tempfile::tempdir()?;
    let deployed = scratch.path().join("mike.deployed");
    std::fs::write(&deployed, "binary")?;
    let unlinked = PathBuf::from(format!("{}{DELETED_SUFFIX}", deployed.display()));
    write_shims(scratch.path(), &unlinked, &["make"])?;
    let shim = std::fs::read_to_string(scratch.path().join("bin/make"))?;
    assert_eq!(
        shim,
        format!(
            "#!/bin/sh\nexec '{}' fleet timed-run 'make' \"$@\"\n",
            deployed.display()
        )
    );
    std::fs::remove_file(&deployed)?;
    assert!(write_shims(scratch.path(), &unlinked, &["make"]).is_err());
    assert_eq!(
        std::fs::read_to_string(scratch.path().join("bin/make"))?,
        shim
    );
    Ok(())
}

#[test]
fn every_declared_program_gets_a_shim_first_on_path() -> std::io::Result<()> {
    let scratch = tempfile::tempdir()?;
    let exe = scratch.path().join("it's mike");
    std::fs::write(&exe, "binary")?;
    write_shims(scratch.path(), &exe, &["make", "npm"])?;
    assert!(scratch.path().join("bin/npm").is_file());
    let shim = std::fs::read_to_string(scratch.path().join("bin/make"))?;
    assert_eq!(
        shim,
        format!(
            "#!/bin/sh\nexec '{}' fleet timed-run 'make' \"$@\"\n",
            exe.display().to_string().replace('\'', "'\\''")
        )
    );
    let bin = scratch.path().join("bin");
    let env_file = scratch.path().join("claude-env");
    let mut settings = json!({});
    append_session_start(&mut settings, &bin.join(ACTIVATE));
    let hook = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{hook}; {hook}; . \"$CLAUDE_ENV_FILE\"; printf '%s' \"$PATH\""
        ))
        .env("PATH", "/usr/bin:/bin")
        .env("CLAUDE_ENV_FILE", &env_file)
        .output()?;
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}:/usr/bin:/bin", bin.display())
    );
    Ok(())
}

#[test]
fn no_session_start_hook_when_the_operator_turned_hooks_off() {
    let mut settings = json!({"disableAllHooks": true});
    append_session_start(&mut settings, Path::new("/s/bin/activate"));
    assert!(settings.get("hooks").is_none());
}

#[test]
fn explicit_browser_policy_is_preserved() {
    let def = FleetDefinition {
        seat_settings: Some(json!({"env": {"BROWSER": "/operator/chosen-helper"}})),
        ..Default::default()
    };
    let settings = effective(&def, &scratch(), "worker-T1");
    assert_eq!(settings["env"]["BROWSER"], "/operator/chosen-helper");
    assert_eq!(settings["env"]["TMPDIR"], "/state/mike/seats/holdfast/tmp");
}
