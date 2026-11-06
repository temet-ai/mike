use std::process::Command;

const CHILD: &str = "MIKE_TEST_FIXTURE_REQUIREMENT_CHILD";

#[test]
fn selected_database_test_refuses_missing_or_blank_admin_url() -> anyhow::Result<()> {
    if std::env::var(CHILD).as_deref() == Ok("1") {
        let runtime = tokio::runtime::Runtime::new()?;
        let result = runtime.block_on(mike::store::test_support::TestDb::bootstrap());
        let error = result
            .err()
            .ok_or_else(|| anyhow::anyhow!("missing DB fixture silently passed"))?;
        assert!(error.to_string().contains("MIKE_TEST_ADMIN_URL"));
        assert!(error.to_string().contains("NOT RUN"));
        return Ok(());
    }
    let home = tempfile::tempdir()?;
    for value in [None, Some(""), Some("  ")] {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "db_fixture_contract::selected_database_test_refuses_missing_or_blank_admin_url",
                "--nocapture",
            ])
            .env_clear()
            .env("HOME", home.path())
            .env(CHILD, "1")
            .current_dir(home.path());
        if let Some(value) = value {
            command.env("MIKE_TEST_ADMIN_URL", value);
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "fixture contract subprocess failed: {output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "child test did not execute: {output:?}"
        );
    }
    Ok(())
}
