use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use mike::launch::fleet::{FleetDefinition, test_seams};
use serde_json::{Value, json};

const GUARD_ENV: &str = "MIKE_FLEET_BROWSER_GUARD";
const HELPER: &str = "#!/bin/sh\nprintf '%s\\000' \"${0##*/}\" \"$@\" >> \"$FIXTURE_LOG\"\nexit \"${FIXTURE_STATUS:-0}\"\n";

struct Fixture {
    root: tempfile::TempDir,
    bin: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("operator's bin");
        std::fs::create_dir(&bin)?;
        for name in ["xdg-open", "gio", "open"] {
            write_executable(&bin.join(name), HELPER)?;
        }
        write_executable(
            &bin.join("fixture-mcp"),
            "#!/bin/sh\nexec xdg-open 'https://fixture.invalid'\n",
        )?;
        let log = root.path().join("helper.log");
        Ok(Self { root, bin, log })
    }

    fn settings(&self, definition: &FleetDefinition, seat: &str) -> anyhow::Result<Value> {
        test_seams::prepared_seat_settings(definition, &self.root.path().join("seat's state"), seat)
            .map_err(anyhow::Error::msg)
    }

    fn command(&self, settings: &Value, shell: &str) -> anyhow::Result<Command> {
        let args = vec![
            "-c".into(),
            shell.into(),
            "fixture".into(),
            "--settings".into(),
            settings.to_string(),
        ];
        let command = test_seams::guarded_terminal_command("/bin/sh", &args);
        let mut child = Command::new(&command[0]);
        child
            .args(&command[1..])
            .env_clear()
            .env("PATH", &self.bin)
            .env("FIXTURE_LOG", &self.log);
        for (name, value) in settings["env"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("missing env"))?
        {
            if let Some(value) = value.as_str() {
                child.env(name, value);
            }
        }
        Ok(child)
    }

    fn run(&self, settings: &Value, shell: &str) -> anyhow::Result<Output> {
        Ok(self.command(settings, shell)?.output()?)
    }
}

fn write_executable(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[test]
fn fleet_child_processes_block_direct_openers_before_mcp_starts() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    for seat in [
        "worker-T1",
        "coordinator",
        "acceptance-2",
        "observer",
        "reference",
        "template-rust",
    ] {
        let settings = fixture.settings(&FleetDefinition::default(), seat)?;
        for command in [
            "xdg-open https://fixture.invalid",
            "gio open https://fixture.invalid",
            "gio launch fixture.desktop",
            "open https://fixture.invalid",
            "fixture-mcp",
        ] {
            let output = fixture.run(&settings, command)?;
            assert_eq!(output.status.code(), Some(126), "{seat}: {command}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("desktop launch blocked"));
            assert!(
                !fixture.log.exists(),
                "fixture desktop helper unexpectedly ran"
            );
        }
    }
    Ok(())
}

#[test]
fn gio_nonlaunch_operations_preserve_arguments_output_and_exit_status() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let settings = fixture.settings(&FleetDefinition::default(), "worker-T1")?;
    let output = fixture
        .command(
            &settings,
            "gio info 'a path; with quotes' --attributes='a,b'",
        )?
        .env("FIXTURE_STATUS", "17")
        .output()?;
    assert_eq!(output.status.code(), Some(17));
    assert!(output.stderr.is_empty());
    assert_eq!(
        std::fs::read(&fixture.log)?,
        b"gio\0info\0a path; with quotes\0--attributes=a,b\0"
    );
    Ok(())
}

#[test]
fn explicit_guard_opt_out_runs_only_fixture_helpers_without_path_changes() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let definition = FleetDefinition {
        seat_settings: Some(json!({"env": {GUARD_ENV: "0", "BROWSER": "/operator/helper"}})),
        ..Default::default()
    };
    let settings = fixture.settings(&definition, "acceptance-2")?;
    assert!(settings["env"].get("PATH").is_none());
    assert_eq!(settings["env"]["BROWSER"], "/operator/helper");
    assert!(
        !fixture
            .root
            .path()
            .join("seat's state/browser-guard")
            .exists()
    );
    assert!(
        fixture
            .run(&settings, "xdg-open https://fixture.invalid")?
            .status
            .success()
    );
    assert_eq!(
        std::fs::read(&fixture.log)?,
        b"xdg-open\0https://fixture.invalid\0"
    );
    Ok(())
}

#[test]
fn browser_guard_retains_runtime_path_and_other_shim_prefixes() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let settings = fixture.settings(&FleetDefinition::default(), "worker-T1")?;
    let cargo = fixture.root.path().join("separate-cargo-shim");
    std::fs::create_dir(&cargo)?;
    let inherited = format!("{}:{}", cargo.display(), fixture.bin.display());
    let output = fixture
        .command(&settings, "printf '%s' \"$PATH\"")?
        .env("PATH", &inherited)
        .output()?;
    assert!(output.status.success());
    let expected = format!(
        "{}:{inherited}",
        fixture
            .root
            .path()
            .join("seat's state/browser-guard")
            .display()
    );
    assert_eq!(String::from_utf8(output.stdout)?, expected);
    Ok(())
}

#[test]
fn session_start_refresh_appends_to_existing_environment_file() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let settings = fixture.settings(&FleetDefinition::default(), "worker-T1")?;
    let env_file = fixture.root.path().join("claude.env");
    std::fs::write(&env_file, "export OPERATOR_VALUE='kept'\n")?;
    let hook = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing SessionStart command"))?;
    let output = Command::new("/bin/sh")
        .args(["-c", hook])
        .env_clear()
        .env("PATH", &fixture.bin)
        .env("CLAUDE_ENV_FILE", &env_file)
        .output()?;
    assert!(output.status.success());
    let script = format!(
        ". '{}' && printf '%s' \"$OPERATOR_VALUE\" && open https://fixture.invalid",
        env_file.display()
    );
    let output = Command::new("/bin/sh")
        .args(["-c", &script])
        .env_clear()
        .env("PATH", &fixture.bin)
        .env("FIXTURE_LOG", &fixture.log)
        .output()?;
    assert_eq!(output.stdout, b"kept");
    assert_eq!(output.status.code(), Some(126));
    assert!(!fixture.log.exists());
    Ok(())
}

#[test]
fn absent_original_gio_reports_failure_without_recursing() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let settings = fixture.settings(&FleetDefinition::default(), "worker-T1")?;
    let empty = fixture.root.path().join("empty");
    std::fs::create_dir(&empty)?;
    let output = fixture
        .command(&settings, "gio info fixture.txt")?
        .env("PATH", empty)
        .output()?;
    assert_eq!(output.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&output.stderr).contains("original gio is unavailable"));
    Ok(())
}

#[test]
fn nested_browser_guards_forward_gio_without_recursing_or_removing_cargo() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let settings = fixture.settings(&FleetDefinition::default(), "worker-T1")?;
    let other = test_seams::prepared_seat_settings(
        &FleetDefinition::default(),
        &fixture.root.path().join("other-seat"),
        "worker-T2",
    )
    .map_err(anyhow::Error::msg)?;
    let first = settings["env"]["MIKE_FLEET_BROWSER_GUARD_BIN"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("first guard"))?;
    let second = other["env"]["MIKE_FLEET_BROWSER_GUARD_BIN"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("second guard"))?;
    let path = format!("{first}:{second}:{}", fixture.bin.display());
    let output = fixture
        .command(&settings, "gio info fixture.txt")?
        .env("PATH", &path)
        .output()?;
    assert!(output.status.success());
    assert_eq!(std::fs::read(&fixture.log)?, b"gio\0info\0fixture.txt\0");
    std::fs::remove_file(&fixture.log)?;
    let output = fixture
        .command(&settings, "xdg-open https://fixture.invalid")?
        .env("PATH", path)
        .env(GUARD_ENV, "0")
        .output()?;
    assert!(output.status.success());
    assert_eq!(
        std::fs::read(&fixture.log)?,
        b"xdg-open\0https://fixture.invalid\0"
    );
    Ok(())
}

#[test]
fn absolute_fixture_launchers_remain_outside_the_documented_path_guard() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let settings = fixture.settings(&FleetDefinition::default(), "worker-T1")?;
    let executable = fixture
        .bin
        .join("open")
        .to_string_lossy()
        .replace('\'', "'\\''");
    let output = fixture.run(
        &settings,
        &format!("'{executable}' https://fixture.invalid"),
    )?;
    assert!(output.status.success());
    assert_eq!(
        std::fs::read(&fixture.log)?,
        b"open\0https://fixture.invalid\0"
    );
    Ok(())
}
