use std::path::Path;
use std::process::{Command, Stdio};

use super::context::{binary_copy, is_elf};

#[test]
fn binary_copy_of_this_process_is_an_executable_outside_proc() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let copy = binary_copy(Path::new("/proc/self/exe"), dir.path()).map_err(anyhow::Error::msg)?;
    assert!(copy.starts_with(dir.path()), "copy at {}", copy.display());
    assert!(is_elf(&copy));
    Ok(())
}

#[test]
fn binary_copy_is_made_once_and_older_copies_are_removed() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("mike-1-2-3"), b"old build")?;
    let first = binary_copy(Path::new("/proc/self/exe"), dir.path()).map_err(anyhow::Error::msg)?;
    let modified = std::fs::metadata(&first)?.modified()?;
    let second =
        binary_copy(Path::new("/proc/self/exe"), dir.path()).map_err(anyhow::Error::msg)?;
    assert_eq!(first, second);
    assert_eq!(std::fs::metadata(&second)?.modified()?, modified);
    assert!(!dir.path().join("mike-1-2-3").exists());
    Ok(())
}

#[test]
fn binary_copy_still_works_after_the_running_file_is_deleted() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let original = dir.path().join("sleeper");
    std::fs::copy("/usr/bin/sleep", &original)?;
    let mut child = Command::new(&original)
        .arg("30")
        .stdout(Stdio::null())
        .spawn()?;
    std::fs::remove_file(&original)?;
    let live = format!("/proc/{}/exe", child.id());
    let made = binary_copy(Path::new(&live), &dir.path().join("bin"));
    child.kill()?;
    child.wait()?;
    let copy = made.map_err(anyhow::Error::msg)?;
    assert!(
        is_elf(&copy),
        "copy at {} must be an executable",
        copy.display()
    );
    Ok(())
}
