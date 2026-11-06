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
