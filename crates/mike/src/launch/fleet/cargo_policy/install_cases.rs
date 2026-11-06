use super::*;
use anyhow::Context;
use std::os::unix::ffi::OsStringExt;

pub fn every_role_and_successor_gets_the_same_default_policy()
-> Result<(), Box<dyn std::error::Error>> {
    for role in [
        "worker",
        "template-rust",
        "coordinator",
        "acceptance",
        "acceptance-2",
        "observer-3",
        "reference-2",
    ] {
        let mut settings = json!({"env": {"MIKE_MCP_PROFILE": role}});
        assert!(matches!(
            configure(&mut settings, Path::new("/private/seat"))?,
            Configured::Prepared(_)
        ));
        assert_eq!(settings["env"][ENABLED_ENV], "1");
        assert_eq!(settings["env"]["MIKE_MCP_PROFILE"], role);
    }
    Ok(())
}

pub fn operator_optout_and_disabled_hooks_preserve_settings()
-> Result<(), Box<dyn std::error::Error>> {
    let mut settings =
        json!({"env": {ENABLED_ENV: "0", "PATH": "/operator"}, "hooks": "untouched"});
    let before = settings.clone();
    let unsupported = PathBuf::from(std::ffi::OsString::from_vec(b"/non-utf8-\xff".to_vec()));
    assert_eq!(
        configure(&mut settings, &unsupported)?,
        Configured::Disabled
    );
    assert_eq!(settings, before);
    let mut settings = json!({"disableAllHooks": true});
    configure(&mut settings, Path::new("/private/seat"))?;
    assert!(settings.get("hooks").is_none());
    assert!(shell_setup(&["--settings".into(), settings.to_string()]).is_some());
    Ok(())
}

pub fn browser_and_operator_prefixes_and_permissions_are_preserved()
-> Result<(), Box<dyn std::error::Error>> {
    let mut settings = json!({"env": {"PATH": "/browser:/operator", "MIKE_FLEET_BROWSER_GUARD": "1"},
        "permissions": {"deny": ["Bash(cargo build:*)"]}, "hooks": {"SessionStart": [{"hooks": []}]}});
    configure(&mut settings, Path::new("/private/seat"))?;
    let once = settings.clone();
    configure(&mut settings, Path::new("/private/seat"))?;
    assert_eq!(settings, once);
    assert_eq!(
        settings["env"]["PATH"],
        "/private/seat/cargo-policy/bin:/browser:/operator"
    );
    assert_eq!(settings["permissions"]["deny"][0], "Bash(cargo build:*)");
    assert_eq!(
        settings["hooks"]["SessionStart"]
            .as_array()
            .context("SessionStart must be an array")?
            .len(),
        2
    );
    Ok(())
}

pub fn malformed_hooks_and_unsupported_paths_fail_transactionally() {
    let mut settings = json!({"hooks": {"SessionStart": {}}});
    let before = settings.clone();
    assert!(configure(&mut settings, Path::new("/private/seat")).is_err());
    assert_eq!(settings, before);
    for path in ["relative", "/private/with:colon", "/private/with\nnewline"] {
        assert!(configure(&mut json!({}), Path::new(path)).is_err());
    }
}
