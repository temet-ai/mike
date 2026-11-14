use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crate::launch::fleet::operation::CountedResource;

fn private_root() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(root)
}

fn resource(capacity: u32) -> ResourceClaim {
    ResourceClaim::Counted(CountedResource {
        name: "shared-build".into(),
        capacity,
    })
}

fn declared() -> TimedCommand {
    TimedCommand {
        subcommands: vec!["build".into(), "test".into(), "nextest".into()],
        ..TimedCommand::default()
    }
}

#[test]
fn declared_subcommands_are_timed_and_every_other_run_is_not() {
    for words in [
        &["build", "-p", "app"][..],
        &["+nightly", "test"],
        &["--manifest-path", "a/manifest", "build"],
        &["nextest", "run", "-p", "gg"],
    ] {
        assert!(declared().measures(words.iter().copied()), "{words:?}");
    }
    for words in [
        &["metadata", "--format-version", "1"][..],
        &["--version"],
        &["fmt", "--", "test"],
    ] {
        assert!(!declared().measures(words.iter().copied()), "{words:?}");
    }
}

#[test]
fn a_program_declared_without_subcommands_is_timed_on_every_run() {
    assert!(TimedCommand::default().measures(["anything"]));
    assert!(TimedCommand::default().measures([]));
}

#[test]
fn the_declaration_comes_from_the_seat_environment_and_only_for_that_program() {
    let text = r#"{"make": {"subcommands": ["all"]}}"#;
    let found = declared_in(text, OsStr::new("make"));
    assert_eq!(
        found.map(|timed| timed.subcommands),
        Some(vec!["all".to_string()])
    );
    assert!(declared_in(text, OsStr::new("npm")).is_none());
    assert!(declared_in("not json", OsStr::new("make")).is_none());
}

#[test]
fn the_shim_folder_is_taken_off_path_so_the_program_resolves_to_the_real_one() {
    let path =
        std::env::join_paths(["/shims", "/usr/bin", "/home/me/.local/bin"]).unwrap_or_default();
    let clean = path_without(&path, Path::new("/shims"));
    let entries: Vec<PathBuf> = std::env::split_paths(&clean).collect();
    assert_eq!(
        entries,
        [
            PathBuf::from("/usr/bin"),
            PathBuf::from("/home/me/.local/bin")
        ]
    );
}

#[test]
fn at_capacity_one_a_second_timed_command_waits_until_the_first_releases() -> anyhow::Result<()> {
    let root = private_root()?;
    let first = admit(root.path(), &resource(1), |_| {}).map_err(anyhow::Error::msg)?;
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let (admitted_tx, admitted_rx) = mpsc::channel();
    let path = root.path().to_path_buf();
    let second = std::thread::spawn(move || {
        let permit = admit(&path, &resource(1), |in_use| {
            let _ = waiting_tx.send(in_use);
        });
        let _ = admitted_tx.send(());
        permit.map(drop)
    });
    assert_eq!(waiting_rx.recv_timeout(Duration::from_secs(10))?, 1);
    assert!(admitted_rx.try_recv().is_err());
    drop(first);
    admitted_rx.recv_timeout(Duration::from_secs(10))?;
    second
        .join()
        .map_err(|_| anyhow::anyhow!("panicked"))?
        .map_err(anyhow::Error::msg)?;
    Ok(())
}

#[test]
fn units_up_to_the_capacity_are_taken_without_waiting() -> anyhow::Result<()> {
    let root = private_root()?;
    let held: Vec<Permit> = (0..3)
        .map(|_| {
            admit(root.path(), &resource(3), |_| {
                panic!("waited with a unit free")
            })
        })
        .collect::<Result<_, _>>()
        .map_err(anyhow::Error::msg)?;
    assert_eq!(held.len(), 3);
    Ok(())
}

#[test]
fn a_timed_command_without_resources_takes_no_unit() {
    assert!(admit_declared(&declared()).is_empty());
}

#[test]
fn a_run_outside_the_declaration_leaves_the_units_as_they_were() -> anyhow::Result<()> {
    let root = private_root()?;
    let held = admit(root.path(), &resource(1), |_| {}).map_err(anyhow::Error::msg)?;
    assert!(!declared().measures(["metadata", "--format-version", "1"]));
    let directory = super::super::operation_admission::directory(root.path(), "shared-build")
        .map_err(anyhow::Error::msg)?;
    let probe = Pending::open_slot(&directory, 0)?;
    assert!(!probe.try_take(&CancellationToken::new())?);
    drop(held);
    assert!(probe.try_take(&CancellationToken::new())?);
    Ok(())
}
