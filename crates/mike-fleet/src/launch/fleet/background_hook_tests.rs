use super::*;

fn commands() -> Vec<String> {
    [
        "cargo build",
        "cargo check",
        "cargo test",
        "cargo nextest run",
    ]
    .map(String::from)
    .to_vec()
}

fn settings_env() -> String {
    json!(commands()).to_string()
}

fn event(command: &str) -> Value {
    json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
    })
}

#[test]
fn declared_commands_and_timeouts_are_background_candidates() {
    for command in [
        "cargo build -p app",
        "cargo check",
        "cargo test -- --nocapture",
        "/home/me/.cargo/bin/cargo build",
        "timeout 1700 cargo nextest run -p gg --test plugin_boundary --test renderer_parity",
        "cd '/repo with spaces' && export RUST_LOG=debug; timeout 580 cargo nextest run -p gg-core -E 'test(/ipc::/)' > /tmp/run.log 2>&1 | tail -10",
        "RUST_LOG=info env CARGO_TARGET_DIR=target timeout 300 command cargo check",
        "cargo \\\ntest",
    ] {
        assert!(syntax::declared_command(command, &commands()), "{command}");
    }
}

#[test]
fn undeclared_commands_and_unsupported_shell_syntax_are_left_unchanged() {
    for command in [
        "cargo metadata --format-version 1",
        "cargo run -- cargo test",
        "cargo install nextest",
        "cargo nextest list",
        "cargo nextest --help",
        "cargo clean",
        "echo 'cargo test'",
        "printf '%s' \"cargo build && cargo test\"",
        "cat <<EOF\ncargo test\nEOF",
        "python3 -c 'import os; os.system(\"cargo test\")'",
        "bash -c 'cargo test'",
        "timeout --help cargo test",
        "sudo cargo test",
        "cargo test$(touch /tmp/no)",
        "echo `cargo metadata`; cargo test",
        "for i in 1; do echo hi; cargo test; done",
        "cargo test # keep foreground",
        "cargo test &",
        "cargo test 'unfinished",
        "cargo test < /tmp/input",
        "scripts/cx test",
        "cargo-test",
        "cargo testing",
    ] {
        assert!(!syntax::declared_command(command, &commands()), "{command}");
        assert!(
            response(&event(command), &commands()).is_none(),
            "{command}"
        );
    }
}

#[test]
fn any_program_can_be_declared_and_multi_word_commands_match_as_a_prefix() {
    let declared = ["npm run build".to_string(), "scripts/slow".to_string()];
    for command in ["npm run build --silent", "timeout 60 scripts/slow x"] {
        assert!(syntax::declared_command(command, &declared), "{command}");
    }
    for command in ["npm run lint", "npm", "other/scripts/slow x", "cargo test"] {
        assert!(!syntax::declared_command(command, &declared), "{command}");
    }
    assert!(!syntax::declared_command("anything", &[]));
    assert!(!syntax::declared_command("anything", &[String::new()]));
}

#[test]
fn backgrounding_preserves_every_other_input_field_and_never_grants_permission()
-> Result<(), String> {
    let mut call = event("cd /tmp/repo && RUST_LOG=debug timeout 580 cargo test > test.log 2>&1");
    call["tool_input"]["timeout"] = json!(1_700_000);
    call["tool_input"]["description"] = json!("Run the requested suite");
    call["tool_input"]["run_in_background"] = json!(false);
    call["tool_input"]["dangerouslyDisableSandbox"] = json!(false);
    call["tool_input"]["future_field"] = json!({"untouched": [1, 2]});
    let answer =
        response(&call, &commands()).ok_or("declared command did not produce an update")?;
    let mut expected = call["tool_input"].clone();
    expected["run_in_background"] = json!(true);
    assert_eq!(answer["hookSpecificOutput"]["updatedInput"], expected);
    assert!(
        answer["hookSpecificOutput"]
            .get("permissionDecision")
            .is_none()
    );
    assert!(answer.get("decision").is_none());
    assert!(
        answer["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .is_some_and(|s| s.contains("not a passed check"))
    );
    Ok(())
}

#[test]
fn unrelated_events_and_already_backgrounded_inputs_are_unchanged() {
    let mut call = event("cargo test");
    call["tool_input"]["run_in_background"] = json!(true);
    assert!(response(&call, &commands()).is_none());
    call["tool_input"]["run_in_background"] = json!(false);
    call["tool_name"] = json!("Read");
    assert!(response(&call, &commands()).is_none());
    call["tool_name"] = json!("Bash");
    call["hook_event_name"] = json!("PostToolUse");
    assert!(response(&call, &commands()).is_none());
    assert!(response(&json!({}), &commands()).is_none());
}

#[test]
fn fleet_identity_and_a_declaration_are_required() {
    let listed = commands();
    assert!(fleet_enabled(Some("fleet-build-a1"), &listed, None));
    for run in [None, Some("manual-run"), Some("fleet-")] {
        assert!(!fleet_enabled(run, &listed, None));
    }
    assert!(!fleet_enabled(Some("fleet-build-a1"), &[], None));
    assert!(!fleet_enabled(Some("fleet-build-a1"), &listed, Some("1")));
}

#[test]
fn operator_hook_entries_are_preserved_and_default_handler_is_idempotent() {
    let original =
        json!({"matcher": "Bash", "hooks": [{"type": "command", "command": "operator-guard"}]});
    let mut settings = json!({
        "env": {COMMANDS_ENV: settings_env(), "CUSTOM": "retained"},
        "hooks": {"PreToolUse": [original.clone()], "Stop": [{"hooks": []}]},
    });
    let before = settings.clone();
    let path = Path::new("/state/it's mike");
    append_hook(&mut settings, path);
    append_hook(&mut settings, path);
    assert_eq!(settings["hooks"]["PreToolUse"][0], original);
    assert_eq!(
        settings["hooks"]["PreToolUse"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(settings["hooks"]["Stop"], before["hooks"]["Stop"]);
    assert_eq!(settings["env"], before["env"]);
    assert_eq!(
        settings["hooks"]["PreToolUse"][1]["hooks"][0]["command"],
        "'/state/it'\\''s mike' fleet background-hook"
    );
}

#[test]
fn explicit_disable_and_invalid_operator_hook_shapes_are_not_overwritten() {
    for mut settings in [
        json!({"disableAllHooks": true, "env": {COMMANDS_ENV: settings_env()}}),
        json!({"env": {COMMANDS_ENV: "[]"}}),
        json!({"env": {COMMANDS_ENV: "not json"}}),
        json!({"env": {COMMANDS_ENV: settings_env(), BACKGROUND_DISABLED: "1"}}),
        json!({"env": {COMMANDS_ENV: settings_env()}, "hooks": "operator-invalid"}),
        json!({"env": {COMMANDS_ENV: settings_env()}, "hooks": {"PreToolUse": "operator-invalid"}}),
        json!({}),
    ] {
        let before = settings.clone();
        append_hook(&mut settings, Path::new("/mike"));
        assert_eq!(settings, before);
    }
}

#[test]
fn malformed_or_oversized_input_fails_without_emitting_a_rewrite() {
    for input in [
        b"not json".to_vec(),
        vec![b' '; MAX_INPUT_BYTES as usize + 1],
    ] {
        let mut output = Vec::new();
        assert!(process(input.as_slice(), &mut output, &commands()).is_err());
        assert!(output.is_empty());
    }
}

#[test]
fn input_and_output_errors_are_propagated() {
    struct Unreadable;
    impl Read for Unreadable {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("unreadable"))
        }
    }
    let mut output = Vec::new();
    assert!(process(Unreadable, &mut output, &commands()).is_err());
    assert!(output.is_empty());
    assert!(process(b"{}".as_slice(), &mut [0_u8; 0][..], &commands()).is_err());
}

#[test]
fn serialized_valid_input_returns_only_a_background_request() -> Result<(), String> {
    let input = serde_json::to_vec(&event("cargo check")).map_err(|e| e.to_string())?;
    let mut output = Vec::new();
    process(input.as_slice(), &mut output, &commands())?;
    let answer: Value = serde_json::from_slice(&output).map_err(|e| e.to_string())?;
    assert_eq!(
        answer["hookSpecificOutput"]["updatedInput"]["run_in_background"],
        true
    );
    Ok(())
}
