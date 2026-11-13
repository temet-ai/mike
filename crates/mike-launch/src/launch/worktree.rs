use std::path::{Path, PathBuf};
use std::time::Duration;

use super::error::LaunchError;

const GIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeKind {
    NotRepo,
    Main,
    Linked { main_root: Option<PathBuf> },
}

pub async fn detect(folder: &Path) -> Result<WorktreeKind, LaunchError> {
    let dot_git = folder.join(".git");
    let git_is_file = std::fs::metadata(&dot_git)
        .map(|m| m.is_file())
        .unwrap_or(false);
    let Some((git_dir, common_dir)) = git_dirs(folder).await? else {
        return Ok(WorktreeKind::NotRepo);
    };
    if !git_is_file && same_dir(folder, &git_dir, &common_dir) {
        return Ok(WorktreeKind::Main);
    }
    let main_root = std::fs::canonicalize(folder.join(&common_dir))
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    Ok(WorktreeKind::Linked { main_root })
}

pub const WORKTREES_DIR: &str = ".claude/worktrees";

pub async fn check(folder: &Path, worktree_ok: bool) -> Result<WorktreeKind, LaunchError> {
    let kind = detect(folder).await?;
    if let (WorktreeKind::Linked { main_root }, false) = (&kind, worktree_ok)
        && !lives_in_repository(folder, main_root.as_deref())
    {
        return Err(LaunchError::Refused(refusal(folder, main_root.as_deref())));
    }
    Ok(kind)
}

pub fn lives_in_repository(folder: &Path, main_root: Option<&Path>) -> bool {
    let Some(root) = main_root else {
        return false;
    };
    let home = root.join(WORKTREES_DIR);
    std::fs::canonicalize(folder).is_ok_and(|f| f.starts_with(&home))
}

fn refusal(folder: &Path, main_root: Option<&Path>) -> String {
    let mut msg = format!(
        "{} is a linked git worktree outside its repository — a cc session there starts without \
         untracked project config (e.g. .mcp.json) and its MCP servers.",
        folder.display()
    );
    if let Some(root) = main_root {
        msg.push_str(&format!(
            "\nworktrees live inside the repository, under {}; spawn at the main repo root instead: {}",
            root.join(WORKTREES_DIR).display(),
            root.display()
        ));
    }
    msg.push_str("\npass --worktree-ok to spawn at this worktree anyway.");
    msg
}

pub const DEFAULT_CARRY_OVER_FILES: [&str; 3] =
    [".mcp.json", ".env", ".claude/settings.local.json"];

pub fn carry_over_files(override_env: Option<&str>) -> Vec<PathBuf> {
    match override_env.filter(|v| !v.trim().is_empty()) {
        Some(value) => value
            .split(':')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect(),
        None => DEFAULT_CARRY_OVER_FILES.iter().map(PathBuf::from).collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarryOverOutcome {
    Linked { relative: PathBuf },
    NoSource { relative: PathBuf },
    AlreadyPresent { relative: PathBuf },
    Failed { relative: PathBuf, detail: String },
}

pub fn carry_over(
    main_root: &Path,
    worktree_folder: &Path,
    files: &[PathBuf],
) -> Vec<CarryOverOutcome> {
    files
        .iter()
        .map(|relative| carry_over_one(main_root, worktree_folder, relative))
        .collect()
}

fn carry_over_one(main_root: &Path, worktree_folder: &Path, relative: &Path) -> CarryOverOutcome {
    let source = main_root.join(relative);
    let dest = worktree_folder.join(relative);
    if std::fs::symlink_metadata(&source).is_err() {
        return CarryOverOutcome::NoSource {
            relative: relative.to_path_buf(),
        };
    }
    if std::fs::symlink_metadata(&dest).is_ok() {
        return CarryOverOutcome::AlreadyPresent {
            relative: relative.to_path_buf(),
        };
    }
    if let Some(parent) = dest.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return CarryOverOutcome::Failed {
            relative: relative.to_path_buf(),
            detail: e.to_string(),
        };
    }
    match std::os::unix::fs::symlink(&source, &dest) {
        Ok(()) => CarryOverOutcome::Linked {
            relative: relative.to_path_buf(),
        },
        Err(e) => CarryOverOutcome::Failed {
            relative: relative.to_path_buf(),
            detail: e.to_string(),
        },
    }
}

fn same_dir(folder: &Path, git_dir: &Path, common_dir: &Path) -> bool {
    let a = std::fs::canonicalize(folder.join(git_dir));
    let b = std::fs::canonicalize(folder.join(common_dir));
    match (a, b) {
        (Ok(a), Ok(b)) => a == b,
        _ => git_dir == common_dir,
    }
}

async fn git_dirs(folder: &Path) -> Result<Option<(PathBuf, PathBuf)>, LaunchError> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["rev-parse", "--git-dir", "--git-common-dir"])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match tokio::time::timeout(GIT_TIMEOUT, output).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(Err(e)) => return Err(LaunchError::io("running git rev-parse", e)),
        Err(_) => {
            return Err(LaunchError::Timeout {
                command: "git rev-parse --git-dir --git-common-dir".into(),
                after: GIT_TIMEOUT,
            });
        }
    };
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    match (lines.next(), lines.next()) {
        (Some(git_dir), Some(common)) => Ok(Some((PathBuf::from(git_dir), PathBuf::from(common)))),
        _ => Err(LaunchError::Malformed {
            command: "git rev-parse --git-dir --git-common-dir".into(),
            detail: format!("expected two lines, got {text:?}"),
        }),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn repo_with_commit() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-q"]);
        git(
            repo.path(),
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "init",
            ],
        );
        repo
    }

    #[tokio::test]
    async fn detect_plain_folder_is_not_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect(dir.path()).await.unwrap(), WorktreeKind::NotRepo);
        assert_eq!(
            check(dir.path(), false).await.unwrap(),
            WorktreeKind::NotRepo
        );
    }

    #[tokio::test]
    async fn detect_main_checkout_is_main() {
        let repo = repo_with_commit();
        assert_eq!(detect(repo.path()).await.unwrap(), WorktreeKind::Main);
    }

    #[tokio::test]
    async fn check_linked_worktree_refused_without_flag_names_main_root() {
        let repo = repo_with_commit();
        let wt = tempfile::tempdir().unwrap();
        let wt_path = wt.path().join("linked");
        git(
            repo.path(),
            &["worktree", "add", "-q", wt_path.to_str().unwrap()],
        );
        let kind = detect(&wt_path).await.unwrap();
        let main_root = std::fs::canonicalize(repo.path()).unwrap();
        assert_eq!(
            kind,
            WorktreeKind::Linked {
                main_root: Some(main_root.clone())
            }
        );
        let err = check(&wt_path, false).await.unwrap_err();
        assert_eq!(err.exit_code(), 1);
        let msg = err.to_string();
        assert!(msg.contains("linked git worktree"), "{msg}");
        assert!(msg.contains(&main_root.display().to_string()), "{msg}");
        assert!(msg.contains("--worktree-ok"), "{msg}");
        assert!(matches!(
            check(&wt_path, true).await.unwrap(),
            WorktreeKind::Linked { .. }
        ));
    }

    #[tokio::test]
    async fn check_linked_worktree_inside_the_repository_is_accepted_without_the_flag() {
        let repo = repo_with_commit();
        let wt_path = repo.path().join(WORKTREES_DIR).join("linked");
        std::fs::create_dir_all(repo.path().join(WORKTREES_DIR)).unwrap();
        git(
            repo.path(),
            &["worktree", "add", "-q", wt_path.to_str().unwrap()],
        );
        let main_root = std::fs::canonicalize(repo.path()).unwrap();
        assert!(lives_in_repository(&wt_path, Some(&main_root)));
        assert!(!lives_in_repository(&wt_path, None));
        assert!(matches!(
            check(&wt_path, false).await.unwrap(),
            WorktreeKind::Linked { .. }
        ));
    }

    #[tokio::test]
    async fn detect_missing_folder_is_not_repo() {
        assert_eq!(
            detect(Path::new("/nonexistent/folder")).await.unwrap(),
            WorktreeKind::NotRepo
        );
    }

    #[test]
    fn carry_over_files_defaults_and_env_override() {
        assert_eq!(
            carry_over_files(None),
            DEFAULT_CARRY_OVER_FILES
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(carry_over_files(Some("")), carry_over_files(None));
        assert_eq!(
            carry_over_files(Some("a.toml:sub/b.json")),
            vec![PathBuf::from("a.toml"), PathBuf::from("sub/b.json")]
        );
    }

    #[test]
    fn carry_over_links_present_files_skips_absent_and_never_overwrites() -> anyhow::Result<()> {
        let main = tempfile::tempdir()?;
        let wt = tempfile::tempdir()?;
        std::fs::write(main.path().join(".mcp.json"), "{}")?;
        std::fs::write(main.path().join(".env"), "X=1")?;
        std::fs::write(wt.path().join(".env"), "already here")?;
        std::fs::create_dir_all(main.path().join(".claude"))?;
        std::fs::write(main.path().join(".claude/settings.local.json"), "{}")?;

        let files = carry_over_files(None);
        let outcomes = carry_over(main.path(), wt.path(), &files);
        assert_eq!(outcomes.len(), 3);
        assert!(
            matches!(&outcomes[0], CarryOverOutcome::Linked { relative } if relative == Path::new(".mcp.json"))
        );
        assert!(matches!(
            &outcomes[1],
            CarryOverOutcome::AlreadyPresent { .. }
        ));
        assert!(matches!(&outcomes[2], CarryOverOutcome::Linked { .. }));

        assert_eq!(
            std::fs::read_to_string(wt.path().join(".env"))?,
            "already here"
        );
        assert_eq!(
            std::fs::read_link(wt.path().join(".mcp.json"))?,
            main.path().join(".mcp.json")
        );
        assert_eq!(
            std::fs::read_to_string(wt.path().join(".claude/settings.local.json"))?,
            "{}"
        );
        Ok(())
    }

    #[test]
    fn carry_over_no_source_is_reported_not_an_error() -> anyhow::Result<()> {
        let main = tempfile::tempdir()?;
        let wt = tempfile::tempdir()?;
        let outcomes = carry_over(main.path(), wt.path(), &[PathBuf::from(".mcp.json")]);
        assert!(matches!(&outcomes[0], CarryOverOutcome::NoSource { .. }));
        Ok(())
    }
}
