use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::mpsc;
use std::time::Duration;

fn args(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

fn private_root() -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    Ok(root)
}

fn resource(capacity: u32) -> CargoResource {
    CargoResource {
        name: "mike-rust-build".into(),
        capacity,
    }
}

#[test]
fn builds_tests_and_lints_are_timed_and_quick_commands_are_not() {
    for words in [
        &["build", "-p", "app"][..],
        &["+nightly", "test"],
        &["--manifest-path", "a/Cargo.toml", "check"],
        &["nextest", "run", "-p", "gg"],
        &["clippy", "--all-targets"],
    ] {
        assert!(heavy(&args(words)), "{words:?}");
    }
    for words in [
        &["metadata", "--format-version", "1"][..],
        &["fmt", "--", "--check"],
        &["--version"],
        &["install", "cargo-nextest"],
        &["fmt", "--", "test"],
    ] {
        assert!(!heavy(&args(words)), "{words:?}");
    }
}

#[test]
fn the_shim_folder_is_taken_off_path_so_cargo_resolves_to_the_real_one() {
    let path =
        std::env::join_paths(["/shims", "/usr/bin", "/home/me/.cargo/bin"]).unwrap_or_default();
    let clean = path_without(&path, Path::new("/shims"));
    let entries: Vec<PathBuf> = std::env::split_paths(&clean).collect();
    assert_eq!(
        entries,
        [
            PathBuf::from("/usr/bin"),
            PathBuf::from("/home/me/.cargo/bin")
        ]
    );
}

#[test]
fn the_declared_cargo_resource_is_the_one_cargo_operations_share() -> anyhow::Result<()> {
    let profile = serde_json::json!({"schema_version": 1, "execution": {"cargo": true, "operations": {
        "check": {"command": "scripts/cx check -p mike", "resources": [{"name": "mike-rust-build", "capacity": 3}]},
        "gate": {"command": "scripts/gate check", "resources": [{"name": "mike-rust-build", "capacity": 3}, "mike-test-database"]},
        "frontend": {"command": "scripts/sb-test", "resources": ["mike-frontend-tests"]}
    }}});
    let found = cargo_resource(profile.to_string().as_bytes()).ok_or(anyhow::anyhow!("none"))?;
    assert_eq!(
        (found.name.as_str(), found.capacity),
        ("mike-rust-build", 3)
    );
    let off = serde_json::json!({"schema_version": 1, "execution": {"cargo": false}});
    assert!(cargo_resource(off.to_string().as_bytes()).is_none());
    assert!(cargo_resource(b"{}").is_none());
    Ok(())
}

#[test]
fn at_capacity_one_a_second_wrapped_command_waits_until_the_first_releases() -> anyhow::Result<()> {
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
fn a_command_that_is_not_heavy_takes_no_unit() -> anyhow::Result<()> {
    let root = private_root()?;
    let held = admit(root.path(), &resource(1), |_| {}).map_err(anyhow::Error::msg)?;
    let quick = args(&["metadata", "--format-version", "1"]);
    assert!(!heavy(&quick));
    let directory = super::super::operation_admission::directory(root.path(), "mike-rust-build")
        .map_err(anyhow::Error::msg)?;
    let probe = Pending::open_slot(&directory, 0)?;
    assert!(!probe.try_take(&CancellationToken::new())?);
    drop(held);
    assert!(probe.try_take(&CancellationToken::new())?);
    Ok(())
}
