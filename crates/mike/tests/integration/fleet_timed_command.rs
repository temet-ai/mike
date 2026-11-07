use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use uuid::Uuid;

const SESSION: &str = "00000000-0000-0000-0000-000000000007";
const RUN: &str = "fleet-timed-1";

fn script(path: &Path, body: &str) -> anyhow::Result<()> {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n"))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

struct Seat {
    root: tempfile::TempDir,
}

impl Seat {
    fn new(program: &str) -> anyhow::Result<Self> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("real"))?;
        std::fs::create_dir_all(root.path().join("shims"))?;
        script(
            &root.path().join("real").join(program),
            "printf 'real %s' \"$*\"; exit 7",
        )?;
        script(
            &root.path().join("shims").join(program),
            "echo shim-recursed; exit 99",
        )?;
        Ok(Self { root })
    }

    fn mike(&self, args: &[&str]) -> Command {
        let dir = self.root.path();
        let mut command = Command::new(env!("CARGO_BIN_EXE_mike"));
        command
            .args(args)
            .env(
                "PATH",
                format!(
                    "{}:{}:/usr/bin:/bin",
                    dir.join("shims").display(),
                    dir.join("real").display()
                ),
            )
            .env("MIKE_FLEET_SHIMS", dir.join("shims"))
            .env("MIKE_FLEET_TIMINGS", dir.join("timings"))
            .env("MIKE_RUN_ID", RUN)
            .env("MIKE_SESSION_ID", SESSION)
            .env_remove("MIKE_FLEET_TIMED_COMMANDS")
            .stdin(Stdio::null());
        command
    }

    fn timings(&self) -> String {
        std::fs::read_to_string(self.root.path().join(format!("timings/{RUN}.jsonl")))
            .unwrap_or_default()
    }
}

fn is_real_run(output: &Output, expected: &str) -> anyhow::Result<()> {
    assert_eq!(String::from_utf8(output.stdout.clone())?, expected);
    assert_eq!(output.status.code(), Some(7));
    Ok(())
}

#[test]
fn a_declared_subcommand_runs_the_real_program_and_records_its_time() -> anyhow::Result<()> {
    let seat = Seat::new("make")?;
    let output = seat
        .mike(&["fleet", "timed-run", "make", "all", "-j2"])
        .env(
            "MIKE_FLEET_TIMED_COMMANDS",
            r#"{"make": {"subcommands": ["all"]}}"#,
        )
        .output()?;
    is_real_run(&output, "real all -j2")?;
    let line: serde_json::Value = serde_json::from_str(seat.timings().trim())?;
    assert_eq!(line["program"], "make");
    assert_eq!(line["args"], "all -j2");
    assert_eq!(line["exit"], 7);
    assert_eq!(
        line["session"].as_str().map(Uuid::parse_str).transpose()?,
        Some(Uuid::parse_str(SESSION)?)
    );
    Ok(())
}

#[test]
fn a_run_the_declaration_does_not_measure_passes_through_unrecorded() -> anyhow::Result<()> {
    let seat = Seat::new("make")?;
    let output = seat
        .mike(&["fleet", "timed-run", "make", "clean"])
        .env(
            "MIKE_FLEET_TIMED_COMMANDS",
            r#"{"make": {"subcommands": ["all"]}}"#,
        )
        .output()?;
    is_real_run(&output, "real clean")?;
    assert!(seat.timings().is_empty());
    Ok(())
}

#[test]
fn a_shim_written_by_an_older_binary_still_reaches_the_real_program() -> anyhow::Result<()> {
    let seat = Seat::new("cargo")?;
    let output = seat
        .mike(&["fleet", "build-slot", "check", "-p", "a"])
        .output()?;
    is_real_run(&output, "real check -p a")?;
    assert!(seat.timings().is_empty());
    Ok(())
}

#[test]
fn a_hook_written_by_an_older_binary_lets_the_tool_call_through() -> anyhow::Result<()> {
    let seat = Seat::new("cargo")?;
    let output = seat
        .mike(&["fleet", "cargo-background-hook"])
        .env_remove("MIKE_FLEET_BACKGROUND_COMMANDS")
        .output()?;
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    Ok(())
}
