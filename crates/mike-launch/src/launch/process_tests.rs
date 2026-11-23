use std::process::{Command, Stdio};

use super::{alive, children, cwd, name};

#[test]
fn alive_is_true_for_this_process_and_false_for_impossible_pids() {
    assert!(alive(std::process::id()));
    assert!(!alive(0));
    assert!(!alive(u32::MAX));
}

#[test]
fn children_lists_a_spawned_child_with_its_name_and_cwd() -> std::io::Result<()> {
    let folder = tempfile::tempdir()?;
    let mut child = Command::new("sleep")
        .arg("5")
        .current_dir(folder.path())
        .stdin(Stdio::null())
        .spawn()?;
    let pid = child.id();

    let listed = children(std::process::id()).contains(&pid);
    let child_name = name(pid);
    let child_cwd = cwd(pid).and_then(|p| p.canonicalize().ok());
    child.kill()?;
    child.wait()?;

    assert!(listed, "the spawned child must be listed");
    assert_eq!(child_name.as_deref(), Some("sleep"));
    assert_eq!(child_cwd, Some(folder.path().canonicalize()?));
    Ok(())
}

#[test]
fn a_reaped_child_is_no_longer_alive() -> std::io::Result<()> {
    let mut child = Command::new("true").spawn()?;
    let pid = child.id();
    child.wait()?;

    assert!(!alive(pid));
    Ok(())
}

#[test]
fn codex_process_is_found_below_a_launcher_without_matching_an_unrelated_process()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let binary = directory.path().join("codex");
    std::fs::copy("/bin/sleep", &binary)?;
    if cfg!(target_os = "macos") {
        let signed = Command::new("codesign")
            .args(["--sign", "-", "--force"])
            .arg(&binary)
            .output()?;
        anyhow::ensure!(signed.status.success(), "fixture codesigning failed");
    }
    let mut launcher = Command::new("sh")
        .args(["-c", "\"$1\" 0.3 & wait", "mike-process-test"])
        .arg(&binary)
        .stdin(Stdio::null())
        .spawn()?;
    let mut found = None;
    for _ in 0..20 {
        found = super::agent_child_of(launcher.id());
        if found.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let found_name = found.and_then(name);
    launcher.wait()?;
    assert_eq!(found_name.as_deref(), Some("codex"));
    assert_eq!(super::agent_child_of(u32::MAX), None);
    Ok(())
}
