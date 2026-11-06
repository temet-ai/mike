use super::*;

pub fn guard_settings_leave_inherited_path_dynamic_and_preserve_operator_hooks() {
    let original = json!({"hooks": [{"type": "command", "command": "operator-hook"}]});
    let mut settings = json!({
        "env": {ENABLED_ENV: "1", "BROWSER": "/operator/helper"},
        "hooks": {"SessionStart": [original.clone()]},
    });
    configure(&mut settings, Path::new("/private/fleet"));
    assert!(settings["env"].get("PATH").is_none());
    assert_eq!(settings["env"]["BROWSER"], "/operator/helper");
    assert_eq!(settings["hooks"]["SessionStart"][0], original);
    assert_eq!(
        settings["hooks"]["SessionStart"].as_array().map(Vec::len),
        Some(2)
    );
}

pub fn guard_opt_out_preserves_operator_path_and_does_not_add_a_hook() {
    let original = json!({"env": {ENABLED_ENV: "0", "PATH": "/chosen/bin"}});
    let mut settings = original.clone();
    configure(&mut settings, Path::new("/private/fleet"));
    assert_eq!(settings, original);
}

pub fn disabled_hooks_keep_prelaunch_containment_and_operator_path() {
    let mut settings = json!({
        "disableAllHooks": true,
        "env": {ENABLED_ENV: "1", "PATH": "/chosen/bin:/other/bin"},
    });
    configure(&mut settings, Path::new("/private/fleet"));
    assert!(settings.get("hooks").is_none());
    assert_eq!(
        settings["env"]["PATH"],
        "/private/fleet/browser-guard:/chosen/bin:/other/bin"
    );
    let args = vec!["--settings".into(), settings.to_string()];
    assert_eq!(
        shell_setup(&args).as_deref(),
        Some(". '/private/fleet/browser-guard/activate'")
    );
}

pub fn explicit_settings_files_and_ordinary_launches_do_not_get_a_guard() {
    for args in [
        vec![],
        vec!["--settings=/operator/settings.json".into()],
        vec!["--settings".into(), "{}".into()],
        vec!["--settings".into()],
    ] {
        assert!(shell_setup(&args).is_none());
        assert_eq!(terminal_command("claude", &args)[0], "claude");
    }
}

#[cfg(unix)]
pub fn browser_guard_refuses_invalid_or_nonprivate_installation_paths() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let scratch = tempfile::tempdir()?;
    assert!(install(Path::new("relative/guard")).is_err());
    assert!(install(&scratch.path().join("invalid:path")).is_err());
    let directory = scratch.path().join("public");
    std::fs::create_dir(&directory)?;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))?;
    assert!(install(&directory).is_err());
    let file = scratch.path().join("file");
    std::fs::write(&file, "not a directory")?;
    assert!(install(&file).is_err());
    Ok(())
}
