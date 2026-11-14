use std::path::Path;
use std::process::Command;

use anyhow::{Result, ensure};
use clap::Parser;

use super::{StructuralCommand, introduced_by, split_range};
use crate::cli::Command as Cli;

const RULES: &str = include_str!("../../../../../.mike/structural/wiring.yml");

#[derive(Parser, Debug)]
struct TestCli {
    #[command(subcommand)]
    command: Option<Cli>,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn commit(root: &Path, files: &[(&str, &str)]) -> Result<String> {
    for (path, content) in files {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, content)?;
    }
    git(root, &["add", "-A"])?;
    git(root, &["commit", "-q", "-m", "step"])?;
    git(root, &["rev-parse", "HEAD"])
}

fn base_repo(root: &Path) -> Result<String> {
    git(root, &["init", "-q"])?;
    commit(
        root,
        &[
            (
                "crates/demo/src/lib.rs",
                "pub fn used() {}\npub fn run() { used(); }\n",
            ),
            ("crates/demo/src/main.rs", "fn main() { demo::run(); }\n"),
        ],
    )
}

#[test]
fn a_commit_adding_a_pub_fn_with_no_caller_reports_one_finding() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let base = base_repo(dir.path())?;
    let head = commit(
        dir.path(),
        &[("crates/demo/src/extra.rs", "pub fn orphan() {}\n")],
    )?;
    let found = introduced_by(dir.path(), &format!("{base}..{head}"), RULES)?;
    let named: Vec<&str> = found.iter().map(|finding| finding.name.as_str()).collect();
    assert_eq!(named, vec!["orphan"]);
    Ok(())
}

#[test]
fn a_commit_adding_the_fn_and_its_caller_reports_nothing() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let base = base_repo(dir.path())?;
    let head = commit(
        dir.path(),
        &[
            ("crates/demo/src/extra.rs", "pub fn adopted() {}\n"),
            (
                "crates/demo/src/main.rs",
                "fn main() { demo::run(); demo::adopted(); }\n",
            ),
        ],
    )?;
    let found = introduced_by(dir.path(), &format!("{base}..{head}"), RULES)?;
    assert!(found.is_empty(), "{found:?}");
    Ok(())
}

#[test]
fn removing_the_only_caller_reports_the_fn_it_orphaned() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let base = base_repo(dir.path())?;
    let head = commit(dir.path(), &[("crates/demo/src/main.rs", "fn main() {}\n")])?;
    let found = introduced_by(dir.path(), &format!("{base}..{head}"), RULES)?;
    let named: Vec<&str> = found.iter().map(|finding| finding.name.as_str()).collect();
    assert_eq!(named, vec!["run"]);
    Ok(())
}

#[test]
fn a_range_without_dots_names_the_input() {
    let err = split_range("abc").err().map(|err| err.to_string());
    assert!(err.is_some_and(|text| text.contains("\"abc\"")));
}

#[test]
fn an_open_ended_range_runs_to_head() -> Result<()> {
    assert_eq!(split_range("abc..")?, ("abc".to_owned(), "HEAD".to_owned()));
    Ok(())
}

#[test]
fn the_wiring_subcommand_parses_its_options() -> Result<()> {
    let parsed =
        TestCli::try_parse_from(["mike", "structural", "wiring", "--range", "a..b", "--json"])?;
    let Some(Cli::Structural(args)) = parsed.command else {
        anyhow::bail!("expected structural");
    };
    let StructuralCommand::Wiring(wiring) = args.command;
    assert_eq!(wiring.range.as_deref(), Some("a..b"));
    assert!(wiring.json);
    Ok(())
}
