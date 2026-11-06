use std::process::Command;

#[test]
fn local_bind_policy_refuses_every_remote_listener_before_database_startup() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    for (flag, address) in [
        ("--proxy-listen-addr", "0.0.0.0:0"),
        ("--inspect-listen-addr", "192.0.2.1:0"),
        ("--spa-bind", "[::]:0"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike"))
            .args([flag, address, "serve"])
            .env_clear()
            .env("HOME", home.path())
            .env("MIKE_DATABASE_URL", "not-a-database-url")
            .current_dir(home.path())
            .output()?;
        assert!(!output.status.success(), "{flag}: {output:?}");
        let error = String::from_utf8(output.stderr)?;
        assert!(error.contains("not loopback"), "{flag}: {error}");
        assert!(
            error.contains("MIKE_UNSUPPORTED_ALLOW_REMOTE_LISTENERS"),
            "{error}"
        );
        assert!(!error.contains("failed to initialize app state"), "{error}");
    }
    Ok(())
}

#[test]
fn local_bind_compatibility_escape_is_explicit_and_printed() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    for (value, expected) in [("0", "false"), ("1", "true")] {
        let output = Command::new(env!("CARGO_BIN_EXE_mike"))
            .args([
                "--unsupported-allow-remote-listeners",
                value,
                "print-config",
                "--shell",
            ])
            .env_clear()
            .env("HOME", home.path())
            .current_dir(home.path())
            .output()?;
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout)?;
        assert!(
            text.contains(&format!(
                "MIKE_UNSUPPORTED_ALLOW_REMOTE_LISTENERS='{expected}'"
            )),
            "{text}"
        );
    }
    Ok(())
}
