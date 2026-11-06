use anyhow::Context;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use super::{entry, install, managed_environment, paths};
use serde_json::json;

struct Fixture {
    root: tempfile::TempDir,
    guard: PathBuf,
    real: PathBuf,
    browser: PathBuf,
    marker: PathBuf,
}

impl Fixture {
    fn new(helper: &Path) -> Result<Self> {
        let root = tempfile::tempdir()?;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
        let guard = root.path().join("cargo-policy/bin");
        install::install(&guard, helper)?;
        let real = root.path().join("real");
        let browser = root.path().join("browser-guard");
        std::fs::create_dir(&real)?;
        std::fs::create_dir(&browser)?;
        script(
            &real.join("rustup"),
            "#!/bin/sh\nprintf ran > \"$FIXTURE_MARKER\"\nprintf '%s\\0' \"${0##*/}\" \"$@\"\nprintf '%s\\0' \"$FIXTURE_SECRET\" \"$PWD\"\n/bin/cat\nexit \"${FIXTURE_EXIT:-0}\"\n",
        )?;
        symlink("rustup", real.join("cargo"))?;
        let marker = root.path().join("ran");
        Ok(Self {
            root,
            guard,
            real,
            browser,
            marker,
        })
    }

    fn path(&self) -> Result<OsString> {
        Ok(std::env::join_paths([
            self.guard.as_path(),
            self.browser.as_path(),
            self.real.as_path(),
            Path::new("/bin"),
        ])?)
    }

    fn command(&self, executable: &Path) -> Result<Command> {
        let mut command = Command::new(executable);
        command
            .current_dir(self.root.path())
            .env("PATH", self.path()?)
            .env("FIXTURE_MARKER", &self.marker)
            .env("FIXTURE_SECRET", "fixture-secret");
        Ok(command)
    }

    fn cargo(&self, args: &[&str]) -> Result<Output> {
        Ok(self
            .command(&self.guard.join("cargo"))?
            .args(args)
            .output()?)
    }
}

fn script(path: &Path, source: &str) -> Result<()> {
    std::fs::write(path, source)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub fn heavy_and_unknown_commands_never_reach_cargo_or_queue(helper: &Path) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    for args in [
        vec!["build"],
        vec!["+nightly", "test"],
        vec!["nextest", "run"],
        vec!["custom-alias"],
        vec!["run", "--bin", "app"],
    ] {
        let result = fixture.cargo(&args)?;
        assert_eq!(result.status.code(), Some(entry::REFUSED));
        assert!(String::from_utf8_lossy(&result.stderr).contains("fleet_check(command)"));
        assert!(!fixture.marker.exists());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("fixture-secret"));
    }
    Ok(())
}

pub fn queries_preserve_toolchain_non_utf8_stdin_environment_cwd_and_exit(
    helper: &Path,
) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let mut child = fixture
        .command(&fixture.guard.join("cargo"))?
        .args([
            OsString::from("+nightly"),
            "metadata".into(),
            OsString::from_vec(b"--filter-platform=\xff".to_vec()),
        ])
        .env("FIXTURE_EXIT", "23")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("fixture value missing")?
        .write_all(b"stdin-fixture\n")?;
    let result = child.wait_with_output()?;
    assert_eq!(result.status.code(), Some(23));
    assert!(
        result
            .stdout
            .starts_with(b"cargo\0+nightly\0metadata\0--filter-platform=\xff\0fixture-secret\0")
    );
    assert!(result.stdout.ends_with(b"stdin-fixture\n"));
    assert!(fixture.marker.exists());
    Ok(())
}

pub fn formatter_and_help_policy_is_explicit(helper: &Path) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let format = fixture.cargo(&["fmt", "--all", "--", "--check"])?;
    assert_eq!(format.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&format.stderr).contains("scoped rustfmt"));
    assert!(!fixture.marker.exists());
    assert!(fixture.cargo(&["help", "build"])?.status.success());
    std::fs::remove_file(&fixture.marker)?;
    assert_eq!(
        fixture.cargo(&["test", "--", "--help"])?.status.code(),
        Some(125)
    );
    assert!(!fixture.marker.exists());
    Ok(())
}

pub fn inherited_policy_is_removed_only_for_explicit_managed_launch(helper: &Path) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let clean = paths::managed_path(&fixture.path()?, fixture.root.path())?
        .context("managed PATH missing")?;
    assert_eq!(
        std::env::split_paths(&clean).collect::<Vec<_>>(),
        vec![
            fixture.browser.clone(),
            fixture.real.clone(),
            PathBuf::from("/bin")
        ]
    );
    let mut command = fixture.command(Path::new("/bin/sh"))?;
    managed_environment::strip_policy_for_owned_command(&mut command, fixture.root.path())?;
    let result = command.args(["-c", "cargo build"]).output()?;
    assert!(result.status.success());
    assert!(result.stdout.starts_with(b"cargo\0build\0"));
    Ok(())
}

pub fn cx_style_wrapper_is_caught_but_absolute_cargo_is_explicitly_uncovered(
    helper: &Path,
) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let wrapper = fixture.root.path().join("cx");
    script(
        &wrapper,
        "#!/bin/sh\nexec cargo --config build.incremental=false \"$@\"\n",
    )?;
    let result = fixture.command(&wrapper)?.arg("check").output()?;
    assert_eq!(result.status.code(), Some(125));
    assert!(!fixture.marker.exists());
    let result = fixture
        .command(&fixture.real.join("cargo"))?
        .arg("build")
        .output()?;
    assert!(result.status.success());
    assert!(fixture.marker.exists());
    Ok(())
}

pub fn session_hook_appends_to_actual_env_file_and_preserves_other_fragments(
    helper: &Path,
) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let mut settings = json!({});
    install::configure(&mut settings, fixture.root.path())?;
    let env_file = fixture.root.path().join("claude-env");
    std::fs::write(&env_file, "export FIRST_FRAGMENT=kept\n")?;
    let hook = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .context("fixture value missing")?;
    assert!(
        fixture
            .command(Path::new("/bin/sh"))?
            .env("CLAUDE_ENV_FILE", &env_file)
            .args(["-c", hook])
            .status()?
            .success()
    );
    let setup = format!(
        ". {}; printf '%s\\n' \"$FIRST_FRAGMENT\" \"$PATH\"",
        install::quote(env_file.to_str().context("fixture value missing")?)
    );
    let result = fixture
        .command(Path::new("/bin/sh"))?
        .env("PATH", &fixture.real)
        .args(["-c", &setup])
        .output()?;
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout)?;
    assert!(text.starts_with("kept\n"));
    assert!(text.contains(fixture.guard.to_str().context("fixture value missing")?));
    assert!(text.contains(fixture.real.to_str().context("fixture value missing")?));
    Ok(())
}

pub fn shell_activation_preserves_browser_prefix_and_handles_quoted_paths(
    helper: &Path,
) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let root = fixture.root.path().join("seat ' with spaces");
    std::fs::create_dir(&root)?;
    let guard = root.join("cargo-policy/bin");
    install::install(&guard, helper)?;
    let setup = format!(
        ". {}; . {}; cargo --version",
        install::quote(
            guard
                .join("activate")
                .to_str()
                .context("fixture value missing")?
        ),
        install::quote(
            guard
                .join("activate")
                .to_str()
                .context("fixture value missing")?
        )
    );
    let result = fixture
        .command(Path::new("/bin/sh"))?
        .args(["-c", &setup])
        .output()?;
    assert!(result.status.success());
    assert!(result.stdout.starts_with(b"cargo\0--version\0"));
    Ok(())
}

pub fn explicit_optout_does_not_install_or_change_operator_path(helper: &Path) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let mut settings = json!({"env": {install::ENABLED_ENV: "0", "PATH": fixture.real.to_str().context("fixture value missing")?}});
    assert_eq!(
        install::configure(&mut settings, fixture.root.path())?,
        install::Configured::Disabled
    );
    assert!(install::shell_setup(&["--settings".into(), settings.to_string()]).is_none());
    assert_eq!(settings["env"]["PATH"].as_str(), fixture.real.to_str());
    assert_eq!(
        OsStr::new("0"),
        OsStr::new(
            settings["env"][install::ENABLED_ENV]
                .as_str()
                .context("fixture value missing")?
        )
    );
    Ok(())
}

pub fn allowed_query_keeps_policy_for_unbudgeted_nested_cargo(helper: &Path) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    script(&fixture.real.join("rustup"), "#!/bin/sh\ncargo build\n")?;
    let result = fixture.cargo(&["metadata"])?;
    assert_eq!(result.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&result.stderr).contains("fleet_check(command)"));
    Ok(())
}

pub fn invalid_or_replaced_policy_marker_refuses_managed_sanitization(helper: &Path) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    std::fs::write(fixture.guard.join(paths::POLICY_MARKER), b"unsupported\n")?;
    assert!(paths::managed_path(&fixture.path()?, fixture.root.path()).is_err());
    assert_eq!(fixture.cargo(&["--version"])?.status.code(), Some(125));
    Ok(())
}

pub fn missing_real_cargo_and_recursive_alias_refuse_without_looping(helper: &Path) -> Result<()> {
    let fixture = Fixture::new(helper)?;
    let missing = fixture
        .command(&fixture.guard.join("cargo"))?
        .env("PATH", &fixture.guard)
        .arg("--version")
        .output()?;
    assert_eq!(missing.status.code(), Some(125));
    let alias = fixture.root.path().join("alias");
    std::fs::create_dir(&alias)?;
    symlink(fixture.guard.join("cargo"), alias.join("cargo"))?;
    let recursive = fixture
        .command(&fixture.guard.join("cargo"))?
        .env("PATH", &alias)
        .arg("--version")
        .output()?;
    assert_eq!(recursive.status.code(), Some(125));
    assert!(!fixture.marker.exists());
    Ok(())
}

pub fn missing_helper_refuses_installation_without_creating_policy_files(
    _helper: &Path,
) -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let directory = root.path().join("cargo-policy/bin");
    assert!(install::install(&directory, &root.path().join("absent-helper")).is_err());
    assert!(!directory.exists());
    Ok(())
}

fn bounded_output(command: Command) -> Result<Output> {
    bounded_output_with_deadline(command, std::time::Duration::from_secs(5))
}

fn bounded_output_with_deadline(
    mut command: Command,
    timeout: std::time::Duration,
) -> Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(child.wait_with_output()?);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Cargo policy fixture child exceeded its bounded deadline");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn replace_with_fifo(path: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::remove_file(path)?;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    Ok(())
}

pub fn fifo_symlink_and_oversized_markers_fail_with_bounded_children(helper: &Path) -> Result<()> {
    for marker in [paths::POLICY_MARKER, ".mike-cargo-shim"] {
        for kind in ["fifo", "symlink", "oversized"] {
            let fixture = Fixture::new(helper)?;
            let path = fixture.guard.join(marker);
            match kind {
                "fifo" => replace_with_fifo(&path)?,
                "symlink" => {
                    std::fs::remove_file(&path)?;
                    symlink(fixture.real.join("rustup"), &path)?;
                }
                _ => std::fs::write(&path, vec![b'x'; 64 * 1024])?,
            }
            let mut command = fixture.command(&fixture.guard.join("cargo"))?;
            command.arg("--version");
            let result = bounded_output(command)?;
            assert_eq!(result.status.code(), Some(125));
            assert!(!fixture.marker.exists());
        }
    }
    Ok(())
}

pub fn unsafe_installation_files_do_not_hang_or_change_data(helper: &Path) -> Result<()> {
    const CHILD: &str = "MIKE_POLICY_INSTALL_FIXTURE_CHILD";
    if let Some(directory) = std::env::var_os(CHILD) {
        let directory = Path::new(&directory);
        assert!(install::install(directory, helper).is_err());
        std::fs::write(directory.join("fixture-checked"), b"checked")?;
        return Ok(());
    }
    for kind in ["fifo", "symlink", "oversized", "conflict", "hardlink"] {
        let fixture = Fixture::new(helper)?;
        let path = fixture.guard.join("activate");
        match kind {
            "fifo" => replace_with_fifo(&path)?,
            "symlink" => {
                std::fs::remove_file(&path)?;
                symlink(fixture.real.join("rustup"), &path)?;
            }
            "hardlink" => {
                std::fs::hard_link(&path, fixture.root.path().join("other-link"))?;
            }
            "oversized" => std::fs::write(&path, vec![b'x'; 64 * 1024])?,
            _ => std::fs::write(&path, b"operator-existing-content\n")?,
        }
        let before = path.symlink_metadata()?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "fleet_cargo_policy::unsafe_installation_files_do_not_hang_or_change_data",
                "--nocapture",
            ])
            .env(CHILD, &fixture.guard);
        let output = bounded_output_with_deadline(command, std::time::Duration::from_secs(20))?;
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            std::fs::read(fixture.guard.join("fixture-checked"))?,
            b"checked"
        );
        let after = path.symlink_metadata()?;
        assert_eq!(before.file_type(), after.file_type());
        assert_eq!(before.permissions().mode(), after.permissions().mode());
        if kind == "conflict" {
            assert_eq!(std::fs::read(&path)?, b"operator-existing-content\n");
        }
    }
    Ok(())
}

pub async fn real_managed_owner_strips_inherited_policy(helper: &Path) -> Result<()> {
    const CHILD: &str = "MIKE_POLICY_MANAGED_FIXTURE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let domain = crate::launch::fleet::test_seams::execution::Domain::new()?;
        let execution = domain.execution("cargo-policy-owned-fixture").await?;
        let command = crate::launch::fleet::test_seams::execution::Command {
            cwd: root.clone(),
            text: "cargo build".into(),
            timeout: std::time::Duration::from_secs(3),
            log: None,
        };
        let (code, output) =
            tokio::time::timeout(std::time::Duration::from_secs(8), execution.spawn(command))
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "managed fixture stalled in {:?}: {:?}",
                        execution.phase(),
                        execution.diagnostic()
                    )
                })??;
        execution.finish().await?;
        assert_eq!(code, Some(0), "{output}");
        assert!(output.starts_with("cargo\0build\0"), "{output}");
        std::fs::write(root.join("managed-checked"), b"checked")?;
        return Ok(());
    }
    let fixture = Fixture::new(helper)?;
    let mut command = fixture.command(&std::env::current_exe()?)?;
    command
        .args([
            "--exact",
            "fleet_cargo_policy::real_managed_owner_strips_inherited_policy",
            "--nocapture",
        ])
        .env(CHILD, fixture.root.path());
    let output = bounded_output_with_deadline(command, std::time::Duration::from_secs(20))?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        std::fs::read(fixture.root.path().join("managed-checked"))?,
        b"checked"
    );
    assert!(fixture.marker.exists());
    Ok(())
}
