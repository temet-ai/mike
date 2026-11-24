use std::path::Path;

use super::{is_transient, top_folder, workspace_folder};

#[test]
fn workspace_folder_with_claude_md_is_found_from_a_subfolder() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let project = home.path().join("Documents/cad/planter-post");
    std::fs::create_dir_all(project.join("parts"))?;
    std::fs::write(project.join("CLAUDE.md"), "# planter")?;
    let claude = home.path().join(".claude");

    let found = workspace_folder(&project.join("parts"), home.path(), &claude);

    assert_eq!(found.as_deref(), Some(project.as_path()));
    Ok(())
}

#[test]
fn workspace_folder_plain_home_folder_is_none() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let plain = home.path().join("Downloads");
    std::fs::create_dir_all(&plain)?;

    assert_eq!(workspace_folder(&plain, home.path(), &home.path().join(".claude")), None);
    Ok(())
}

#[test]
fn workspace_folder_claude_config_dir_itself_is_skipped() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let claude = home.path().join(".claude");
    std::fs::create_dir_all(&claude)?;
    std::fs::write(claude.join("CLAUDE.md"), "global")?;

    assert_eq!(workspace_folder(&claude, home.path(), &claude), None);
    Ok(())
}

#[test]
fn workspace_folder_home_itself_is_none() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    std::fs::write(home.path().join("CLAUDE.md"), "home")?;

    assert_eq!(workspace_folder(home.path(), home.path(), &home.path().join(".claude")), None);
    Ok(())
}

#[test]
fn is_transient_cache_state_and_warm_paths_are_transient() {
    assert!(is_transient(Path::new("/home/u/.claude/.cache/tmp/x")));
    assert!(is_transient(Path::new("/home/u/.local/state/ask-kit/mike/warm/cwd")));
    assert!(is_transient(Path::new("/home/u/x/warm/cwd")));
    assert!(!is_transient(Path::new("/home/u/Documents/cad")));
}

#[test]
fn top_folder_names_the_first_folder_under_home() {
    let home = Path::new("/home/u");
    assert_eq!(
        top_folder(Path::new("/home/u/projects/temet/trainer"), home).as_deref(),
        Some(Path::new("/home/u/projects"))
    );
    assert_eq!(top_folder(home, home), None);
}
