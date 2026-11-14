use std::path::Path;
use std::process::{Command, Stdio};

use super::context::{is_elf, running_binary};

#[test]
fn running_binary_of_this_process_is_its_live_executable() {
    let installed = Path::new("/nonexistent/mike");
    let found = running_binary(std::process::id(), installed);
    assert_eq!(
        found,
        Path::new(&format!("/proc/{}/exe", std::process::id()))
    );
    assert!(is_elf(&found));
}

#[test]
fn running_binary_still_opens_after_its_file_is_replaced() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let copy = dir.path().join("sleeper");
    std::fs::copy("/usr/bin/sleep", &copy)?;
    let mut child = Command::new(&copy)
        .arg("30")
        .stdout(Stdio::null())
        .spawn()?;
    std::fs::remove_file(&copy)?;
    let found = running_binary(child.id(), &copy);
    let opened = is_elf(&found);
    child.kill()?;
    child.wait()?;
    assert!(found.starts_with("/proc"), "found {}", found.display());
    assert!(
        opened,
        "the deleted binary must still open through {}",
        found.display()
    );
    Ok(())
}

#[test]
fn running_binary_without_a_live_process_falls_back_to_the_installed_path() {
    let installed = Path::new("/usr/bin/mike");
    assert_eq!(running_binary(u32::MAX, installed), installed);
}
