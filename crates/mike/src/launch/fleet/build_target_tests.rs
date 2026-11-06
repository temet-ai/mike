use super::directory;

#[test]
fn literal_target_directories_are_normalized_across_supported_wrappers() {
    for command in [
        "CARGO_TARGET_DIR=target/fleet cargo check",
        "env CARGO_TARGET_DIR=./target/fleet/ cargo check",
        "CARGO_TARGET_DIR='target/fleet' timeout 580 cargo nextest run",
        "time -p env CARGO_TARGET_DIR=target/fleet nice -n 5 scripts/cx check",
        "make CARGO_TARGET_DIR=target/fleet test",
        "cargo check --target-dir target/fleet",
        "cargo check --target-dir=./target/fleet/",
    ] {
        assert_eq!(
            directory(command).as_deref(),
            Some("target/fleet"),
            "{command}"
        );
    }
}

#[test]
fn cargo_target_option_overrides_environment_and_test_arguments_do_not() {
    assert_eq!(
        directory("CARGO_TARGET_DIR=target/old cargo test --target-dir target/new").as_deref(),
        Some("target/new")
    );
    assert_eq!(
        directory("cargo test -- --target-dir target/test-argument").as_deref(),
        Some("target")
    );
}

#[test]
fn absent_target_and_explicit_default_share_a_group() {
    for command in [
        "cargo check",
        "scripts/cx test",
        "make test",
        "cargo check --target-dir ./target/",
    ] {
        assert_eq!(directory(command).as_deref(), Some("target"), "{command}");
    }
}

#[test]
fn quoted_paths_are_kept_as_one_directory() {
    assert_eq!(
        directory("CARGO_TARGET_DIR='target/fleet builds' cargo check").as_deref(),
        Some("target/fleet builds")
    );
    assert_eq!(
        directory("cargo check --target-dir '/tmp/fleet builds'").as_deref(),
        Some("/tmp/fleet builds")
    );
}

#[test]
fn dynamic_targets_cwd_changes_and_unknown_wrappers_are_not_guessed() {
    for command in [
        "CARGO_TARGET_DIR=$TARGET cargo check",
        "CARGO_TARGET_DIR=\"$(pwd)/target\" cargo check",
        "cd .claude/worktrees/task1 && cargo check",
        "cargo -C .claude/worktrees/task1 check",
        "cargo check --manifest-path other/Cargo.toml",
        "cargo check --config build.target-dir='other'",
        "bash -c 'cargo check'",
        "scripts/unknown-wrapper cargo check",
        "export CARGO_TARGET_DIR=target/one; cargo check",
        "CARGO_TARGET_DIR=target/one cargo check && CARGO_TARGET_DIR=target/two cargo test",
        "cargo check --target-dir",
        "cargo check # CARGO_TARGET_DIR=target/other",
        "cargo check --target-dir '' --release",
        "make -Cother test",
        "make --directory=other test",
        "cargo check --target-dir=''",
        "CARGO_TARGET_DIR=../target cargo check",
        "CARGO_TARGET_DIR=~/target cargo check",
        "cargo check --target-dir 'unterminated",
    ] {
        assert!(directory(command).is_none(), "{command}");
    }
}
