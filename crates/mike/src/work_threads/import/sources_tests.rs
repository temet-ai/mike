use std::path::Path;
use std::time::{Duration, SystemTime};

use super::find_worklogs;

fn write_worklog(root: &Path, relative: &str, age: Duration) -> std::io::Result<()> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, relative)?;
    let file = std::fs::File::options().write(true).open(&path)?;
    file.set_modified(SystemTime::now() - age)
}

#[test]
fn find_worklogs_reads_threads_that_exist_only_in_a_worktree() -> std::io::Result<()> {
    let repo = tempfile::tempdir()?;
    write_worklog(
        repo.path(),
        "artifacts/threads/deploy/worklog.md",
        Duration::from_secs(60),
    )?;
    write_worklog(
        repo.path(),
        ".claude/worktrees/design/artifacts/threads/design/worklog.md",
        Duration::from_secs(60),
    )?;

    let mut slugs: Vec<String> = find_worklogs(repo.path())
        .into_iter()
        .map(|file| file.slug)
        .collect();
    slugs.sort();

    assert_eq!(slugs, vec!["deploy".to_owned(), "design".to_owned()]);
    Ok(())
}

#[test]
fn find_worklogs_keeps_the_most_recently_changed_copy_of_a_thread() -> std::io::Result<()> {
    let repo = tempfile::tempdir()?;
    write_worklog(
        repo.path(),
        "artifacts/threads/design/worklog.md",
        Duration::from_secs(3600),
    )?;
    write_worklog(
        repo.path(),
        ".claude/worktrees/a/artifacts/threads/design/worklog.md",
        Duration::from_secs(1800),
    )?;
    write_worklog(
        repo.path(),
        ".claude/worktrees/b/artifacts/threads/design/worklog.md",
        Duration::from_secs(60),
    )?;

    let files = find_worklogs(repo.path());

    assert_eq!(files.len(), 1);
    assert!(
        files[0]
            .path
            .ends_with(".claude/worktrees/b/artifacts/threads/design/worklog.md")
    );
    Ok(())
}

#[test]
fn find_worklogs_returns_nothing_for_a_folder_without_threads() -> std::io::Result<()> {
    let repo = tempfile::tempdir()?;

    assert!(find_worklogs(repo.path()).is_empty());
    Ok(())
}
