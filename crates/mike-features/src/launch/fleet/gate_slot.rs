use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "gate_isolation.rs"]
pub mod isolated;

#[path = "check_queue.rs"]
pub mod queue;

pub const GATE_TIMEOUT: Duration = Duration::from_secs(7200);
const SLOTS_DIR: &str = ".claude/worktrees";
const SLOT_NAME: &str = "mike-gate";
const CHECK_NAME: &str = "mike-check";
pub const CHECK_SLOT_NAMES: [&str; 2] = [CHECK_NAME, "mike-check-2"];
const EXCLUDE_LINE: &str = ".claude/worktrees/";
pub const OUTPUT_FILE: &str = ".claude/worktrees/mike-gate-output.log";

fn inside(repo: &str, name: &str) -> Option<PathBuf> {
    let path = Path::new(repo);
    path.file_name()?;
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some(path.join(SLOTS_DIR).join(name))
}

pub fn slot_for(repo: &str) -> Option<PathBuf> {
    inside(repo, SLOT_NAME)
}

pub fn check_slot_for(repo: &str) -> Option<PathBuf> {
    inside(repo, CHECK_NAME)
}

pub fn check_slot_names(commit_check: bool) -> &'static [&'static str] {
    if commit_check {
        &CHECK_SLOT_NAMES
    } else {
        &CHECK_SLOT_NAMES[1..]
    }
}

pub const KEEP_WARM_REASON: &str = "mike's fleet check slot: kept between fleets so its build stays warm; mike resets it before every check";

pub async fn keep_warm(repo: &Path, slot: &Path) {
    let shown = slot.to_string_lossy();
    let args = ["worktree", "lock", "--reason", KEEP_WARM_REASON, &shown];
    match super::owned_git::run_plain(repo, &args).await {
        Ok(_) => {}
        Err(why) if why.contains("already locked") => {}
        Err(why) => {
            tracing::warn!(slot = %shown, %why, "fleet: the check slot could not be locked, so worktree clean-up may remove its warm build");
        }
    }
}

pub fn managed_slot_name(name: &str) -> bool {
    name == SLOT_NAME || CHECK_SLOT_NAMES.contains(&name)
}

pub async fn exclude_slots(repo: &Path) {
    if let Err(why) = write_exclude(repo).await {
        tracing::warn!(repo = %repo.display(), %why, "fleet: mike's files under .claude/worktrees could not be hidden from git status; add `.claude/worktrees/` to .git/info/exclude by hand");
    }
}

async fn write_exclude(repo: &Path) -> Result<(), String> {
    let common = super::owned_git::run_plain(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await?;
    super::managed_command::preparation_allowed()?;
    let common = PathBuf::from(common);
    let exclude = common.join("info").join("exclude");
    let known = std::fs::read_to_string(&exclude).unwrap_or_default();
    let listed = known.lines().any(|line| {
        line.trim() == EXCLUDE_LINE || line.trim() == EXCLUDE_LINE.trim_end_matches('/')
    });
    if listed {
        return Ok(());
    }
    if let Some(parent) = exclude.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let newline = if known.is_empty() || known.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    std::fs::write(&exclude, format!("{known}{newline}{EXCLUDE_LINE}\n")).map_err(|e| e.to_string())
}

pub fn managed_directory(repo: &Path) -> Result<PathBuf, String> {
    let mut directory = std::fs::canonicalize(repo).map_err(|err| err.to_string())?;
    for part in [".claude", "worktrees"] {
        directory.push(part);
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "managed directory `{}` is not an ordinary directory; no files were changed",
                    directory.display()
                ));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&directory).map_err(|err| err.to_string())?;
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(directory)
}

pub async fn prepare(repo: &str) -> Result<(), String> {
    super::managed_command::preparation_allowed()?;
    let repo = Path::new(repo);
    managed_directory(repo)?;
    exclude_slots(repo).await;
    super::managed_command::preparation_allowed()
}

#[cfg(test)]
pub async fn run_gate(slot: &Path, command: &str, run: &str) -> (Option<i32>, String) {
    let _held = queue::hold(slot).await;
    process::run(
        slot,
        command,
        run,
        (GATE_TIMEOUT, "the gate"),
        None,
        Some(super::process_tripwire::Setting::Gate),
    )
    .await
}

pub async fn run_check(
    slot: &Path,
    command: &str,
    run: &str,
    budget: (Duration, &str),
    configured: bool,
) -> (Option<i32>, String) {
    let setting = configured.then_some(super::process_tripwire::Setting::Check);
    process::run(slot, command, run, budget, None, setting).await
}

pub async fn run_gate_logged(
    slot: &Path,
    command: &str,
    run: &str,
    log_path: &Path,
) -> (Option<i32>, String) {
    process::run(
        slot,
        command,
        run,
        (GATE_TIMEOUT, "the gate"),
        Some(log_path),
        Some(super::process_tripwire::Setting::Gate),
    )
    .await
}

#[path = "gate_process.rs"]
pub mod process;

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::launch::fleet::managed_test_support as managed;

    async fn said(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .await?;
        anyhow::ensure!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[tokio::test]
    async fn explicit_commands_use_the_shared_build_and_leave_every_seats_edit_in_place()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let repo = root.path().join("project");
        std::fs::create_dir_all(&repo)?;
        said(&repo, &["init", "-q", "-b", "main"]).await?;
        said(&repo, &["config", "user.email", "t@example.com"]).await?;
        said(&repo, &["config", "user.name", "t"]).await?;
        std::fs::write(repo.join("a.txt"), "committed\n")?;
        std::fs::write(repo.join(".gitignore"), "target/\n")?;
        said(&repo, &["add", "-A"]).await?;
        said(&repo, &["commit", "-q", "-m", "base"]).await?;
        std::fs::create_dir_all(repo.join("target"))?;
        std::fs::write(repo.join("target/built"), "warm\n")?;
        std::fs::write(repo.join("a.txt"), "a seat is still writing this\n")?;
        let repo_text = repo.to_string_lossy().into_owned();
        let slot = repo.clone();
        assert_eq!(slot, repo);

        prepare(&repo_text).await.map_err(anyhow::Error::msg)?;
        let (code, output) = managed::with_context(
            "fleet-x",
            run_check(
                &slot,
                "cat a.txt target/built",
                "fleet-x",
                (GATE_TIMEOUT, "the command"),
                false,
            ),
        )
        .await?;
        assert_eq!(
            (code, output.as_str()),
            (Some(0), "a seat is still writing this\nwarm\n")
        );
        std::fs::write(slot.join(OUTPUT_FILE), &output)?;
        let status = said(&repo, &["status", "--porcelain"]).await?;
        assert_eq!(
            status.lines().collect::<Vec<_>>(),
            vec!["M a.txt"],
            "the seat's edit stays and mike's log is hidden from git status"
        );
        assert!(!check_slot_for(&repo_text).is_some_and(|p| p.exists()));
        Ok(())
    }

    #[tokio::test]
    async fn a_gate_that_fails_reports_its_exit_code_and_output() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (code, output) = managed::with_context(
            "fleet-x",
            run_gate(dir.path(), "echo broken; exit 3", "fleet-x"),
        )
        .await?;
        assert_eq!((code, output.as_str()), (Some(3), "broken\n"));
        Ok(())
    }

    #[test]
    fn floors_and_rulings_never_take_the_slot_kept_for_the_commit_check() {
        assert_eq!(check_slot_names(true), &CHECK_SLOT_NAMES);
        assert!(!check_slot_names(false).contains(&CHECK_NAME));
        assert!(!check_slot_names(false).is_empty());
    }

    #[tokio::test]
    async fn a_check_slot_is_locked_so_worktree_clean_up_keeps_its_warm_build() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        let repo = root.path().join("project");
        std::fs::create_dir_all(&repo)?;
        said(&repo, &["init", "-q", "-b", "main"]).await?;
        said(&repo, &["config", "user.email", "t@example.com"]).await?;
        said(&repo, &["config", "user.name", "t"]).await?;
        std::fs::write(repo.join("a.txt"), "committed\n")?;
        said(&repo, &["add", "-A"]).await?;
        said(&repo, &["commit", "-q", "-m", "base"]).await?;
        let slot = check_slot_for(&repo.to_string_lossy())
            .ok_or_else(|| anyhow::anyhow!("no check slot"))?;
        let shown = slot.to_string_lossy().into_owned();
        said(
            &repo,
            &["worktree", "add", "-q", "--detach", &shown, "HEAD"],
        )
        .await?;
        std::fs::create_dir_all(slot.join("target/fleet-check"))?;
        std::fs::write(slot.join("target/fleet-check/built"), "warm\n")?;

        keep_warm(&repo, &slot).await;
        keep_warm(&repo, &slot).await;

        let listed = said(&repo, &["worktree", "list", "--porcelain"]).await?;
        assert!(
            listed.contains(&format!("locked {KEEP_WARM_REASON}")),
            "{listed}"
        );
        let removed = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "remove", "--force", &shown])
            .output()
            .await?;
        assert!(
            !removed.status.success(),
            "a locked slot must survive worktree remove --force"
        );
        said(&repo, &["worktree", "prune"]).await?;
        assert!(slot.join("target/fleet-check/built").is_file());
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn checkout_aliases_keep_one_slot_identity_before_and_after_creation() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let repo = root.path().join("repo");
        let alias = root.path().join("alias");
        std::fs::create_dir_all(&repo)?;
        std::os::unix::fs::symlink(&repo, &alias)?;
        let slot = slot_for(&repo.to_string_lossy()).ok_or_else(|| anyhow::anyhow!("no slot"))?;
        assert_eq!(slot_for(&alias.to_string_lossy()), Some(slot.clone()));
        std::fs::create_dir_all(&slot)?;
        assert_eq!(slot_for(&alias.to_string_lossy()), Some(slot));
        Ok(())
    }
}
