use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

pub const PUSH_ATTEMPTS: usize = 4;
pub const PUSH_RETRY: Duration = Duration::from_millis(800);
const PUSH_TIMEOUT: Duration = Duration::from_secs(20);
const CREDENTIAL_HELPER: &str = "credential.helper=!gh auth git-credential";
const NO_CREDENTIALS: [&str; 3] = [
    "could not read Username",
    "Authentication failed",
    "Permission denied",
];
const BEHIND_ORIGIN: [&str; 3] = ["fetch first", "non-fast-forward", "[rejected]"];

const DRY_RUN_TIMEOUT: Duration = Duration::from_secs(15);
const GH_STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const PUSHABLE_REMEMBERED_FOR: Duration = Duration::from_secs(60);
const LOCAL_HELPER_KEY: &str = "credential.https://github.com.helper";
const LOCAL_HELPER_VALUE: &str = "!gh auth git-credential";
pub const PUSH_FAILED_KEY: &str = "push-failed";

pub async fn git_said(repo: &str, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", CREDENTIAL_HELPER])
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("git {} could not run: {e}", args.join(" ")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub async fn push_once(repo: &str, branch: &str) -> Result<(), String> {
    git_said(repo, &["push", "--no-verify", "-q", "origin", branch])
        .await
        .map(|_| ())
}

pub async fn fetch_origin(repo: &str, branch: &str) -> Result<(), String> {
    git_said(repo, &["fetch", "-q", "origin", branch])
        .await
        .map(|_| ())
}

pub async fn merge_origin(repo: &str, branch: &str) -> Result<bool, String> {
    let theirs = format!("origin/{branch}");
    if git_said(repo, &["merge-base", "--is-ancestor", &theirs, "HEAD"])
        .await
        .is_ok()
    {
        return Ok(false);
    }
    match git_said(repo, &["merge", "-q", "--no-edit", &theirs]).await {
        Ok(_) => Ok(true),
        Err(why) => {
            let _ = git_said(repo, &["merge", "--abort"]).await;
            Err(format!(
                "{theirs} holds commits this checkout does not, and they could not be merged in: {why}"
            ))
        }
    }
}

async fn push_unbounded(repo: &str, branch: &str) -> Result<(), String> {
    let first = match push_once(repo, branch).await {
        Ok(()) => return Ok(()),
        Err(why) => why,
    };
    if will_not_pass_on_a_retry(&first) || fetch_origin(repo, branch).await.is_err() {
        return Err(first);
    }
    merge_origin(repo, branch).await?;
    push_once(repo, branch).await
}

async fn push_with_timeout(repo: &str, branch: &str, timeout: Duration) -> Result<(), String> {
    tokio::time::timeout(timeout, push_unbounded(repo, branch))
        .await
        .unwrap_or_else(|_| {
            Err(format!(
                "git push timed out after {} ms",
                timeout.as_millis()
            ))
        })
}

pub async fn push(repo: &str, branch: &str) -> Result<(), String> {
    push_with_timeout(repo, branch, PUSH_TIMEOUT).await
}

pub async fn dry_run(repo: &str, branch: &str) -> Result<(), String> {
    let args = ["push", "--no-verify", "--dry-run", "-q", "origin", branch];
    let attempt = git_said(repo, &args);
    match tokio::time::timeout(DRY_RUN_TIMEOUT, attempt).await {
        Ok(result) => result.map(|_| ()),
        Err(_) => Err(format!(
            "git push --dry-run timed out after {} ms",
            DRY_RUN_TIMEOUT.as_millis()
        )),
    }
}

pub async fn gh_logged_in() -> bool {
    let status = tokio::process::Command::new("gh")
        .args(["auth", "status"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(GH_STATUS_TIMEOUT, status).await {
        Ok(output) => output.is_ok_and(|output| output.status.success()),
        Err(_) => {
            tracing::warn!(
                timeout_ms = GH_STATUS_TIMEOUT.as_millis(),
                "gh auth status timed out; check what gh resolves to on the proxy's PATH"
            );
            false
        }
    }
}

async fn set_local_helper(repo: &str) -> Result<(), String> {
    git_said(
        repo,
        &["config", "--local", LOCAL_HELPER_KEY, LOCAL_HELPER_VALUE],
    )
    .await
    .map(|_| ())
}

pub async fn check_pushable_with(
    repo: &str,
    branch: &str,
    gh_is_logged_in: bool,
) -> Result<(), String> {
    match dry_run(repo, branch).await {
        Ok(()) => Ok(()),
        Err(first) => retry_with_helper(repo, branch, first, gh_is_logged_in).await,
    }
}

pub async fn retry_with_helper(
    repo: &str,
    branch: &str,
    first: String,
    gh_is_logged_in: bool,
) -> Result<(), String> {
    if !will_not_pass_on_a_retry(&first) || !gh_is_logged_in {
        return Err(first);
    }
    set_local_helper(repo).await?;
    dry_run(repo, branch).await
}

type Verdicts = dashmap::DashMap<String, (std::time::Instant, Result<(), String>)>;

fn remembered() -> &'static Verdicts {
    static REMEMBERED: std::sync::OnceLock<Verdicts> = std::sync::OnceLock::new();
    REMEMBERED.get_or_init(Verdicts::new)
}

pub async fn check_pushable(repo: &str, branch: &str) -> Result<(), String> {
    let key = format!("{repo}\n{branch}");
    if let Some(seen) = remembered().get(&key)
        && seen.0.elapsed() < PUSHABLE_REMEMBERED_FOR
    {
        return seen.1.clone();
    }
    let verdict = check_pushable_with(repo, branch, gh_logged_in().await).await;
    remembered().insert(key, (std::time::Instant::now(), verdict.clone()));
    verdict
}

pub async fn tell_coordinator(
    persist: &crate::store::persistence::PersistHandle,
    def: &super::definition::FleetDefinition,
    run: &str,
    words: &str,
) {
    let Some(seats) = super::plan::ControlSeats::recorded(def) else {
        return;
    };
    if super::told::already(persist, run, PUSH_FAILED_KEY).await {
        return;
    }
    let body = format!(
        "[mike push] a push of branch `{}` from {} failed, and every fleet_commit answer carries `push_error` until it is fixed. git said: {words}",
        def.branch, def.repo
    );
    match persist
        .launch()
        .insert_message(uuid::Uuid::new_v4(), seats.coordinator, None, &body)
        .await
    {
        Ok(_) => super::told::mark(persist, run, &[PUSH_FAILED_KEY.to_string()]).await,
        Err(err) => {
            tracing::warn!(%run, error = %err, "fleet: could not tell the coordinator a push failed");
        }
    }
}

pub fn will_not_pass_on_a_retry(git_said: &str) -> bool {
    NO_CREDENTIALS.iter().any(|sign| git_said.contains(sign))
}

pub fn behind_origin(git_said: &str) -> bool {
    BEHIND_ORIGIN.iter().any(|sign| git_said.contains(sign))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fleet_git_uses_the_gh_credential_helper_and_bounds_pushes() {
        assert_eq!(
            CREDENTIAL_HELPER,
            "credential.helper=!gh auth git-credential"
        );
        assert!(PUSH_TIMEOUT < Duration::from_secs(120));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_stalled_push_returns_a_timeout_error() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let s = scratch().await?;
        commit_file(&s.fleet, "slow.txt", "wait\n").await?;
        let hook = s._root.path().join("origin.git/hooks/pre-receive");
        std::fs::write(&hook, "#!/bin/sh\nsleep 5\n")?;
        let mut permissions = std::fs::metadata(&hook)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&hook, permissions)?;
        let repo = s.fleet.to_string_lossy();
        let result = push_with_timeout(&repo, "main", Duration::from_millis(50)).await;
        assert_eq!(result, Err("git push timed out after 50 ms".to_string()));
        Ok(())
    }

    #[test]
    fn a_refusal_for_being_behind_is_told_from_one_for_no_credentials() {
        let behind = " ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs to 'origin'\nhint: Updates were rejected because the remote contains work that you do not have locally.";
        assert!(behind_origin(behind));
        assert!(!will_not_pass_on_a_retry(behind));
        let locked_out =
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled";
        assert!(will_not_pass_on_a_retry(locked_out));
        assert!(!behind_origin(locked_out));
        assert!(!behind_origin(
            "fatal: unable to access: Could not resolve host"
        ));
    }

    async fn said(dir: &std::path::Path, args: &[&str]) -> anyhow::Result<String> {
        git_said(&dir.to_string_lossy(), args)
            .await
            .map_err(|why| anyhow::anyhow!("git {args:?}: {why}"))
    }

    struct Scratch {
        _root: tempfile::TempDir,
        fleet: std::path::PathBuf,
        outside: std::path::PathBuf,
    }

    async fn checkout(
        root: &std::path::Path,
        name: &str,
        origin: &str,
    ) -> anyhow::Result<std::path::PathBuf> {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir)?;
        said(&dir, &["init", "-q", "-b", "main"]).await?;
        said(&dir, &["config", "user.email", "t@example.com"]).await?;
        said(&dir, &["config", "user.name", "t"]).await?;
        said(&dir, &["remote", "add", "origin", origin]).await?;
        Ok(dir)
    }

    async fn scratch() -> anyhow::Result<Scratch> {
        let root = tempfile::tempdir()?;
        let origin = root.path().join("origin.git");
        std::fs::create_dir_all(&origin)?;
        said(&origin, &["init", "-q", "--bare", "-b", "main"]).await?;
        let origin_url = origin.to_string_lossy().into_owned();
        let fleet = checkout(root.path(), "fleet", &origin_url).await?;
        let outside = checkout(root.path(), "outside", &origin_url).await?;
        commit_file(&fleet, "a.txt", "one\n").await?;
        said(&fleet, &["push", "-q", "origin", "main"]).await?;
        said(&outside, &["pull", "-q", "origin", "main"]).await?;
        Ok(Scratch {
            _root: root,
            fleet,
            outside,
        })
    }

    async fn commit_file(dir: &std::path::Path, file: &str, text: &str) -> anyhow::Result<()> {
        if let Some(parent) = dir.join(file).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(dir.join(file), text)?;
        said(dir, &["add", file]).await?;
        said(dir, &["commit", "-q", "-m", file]).await?;
        Ok(())
    }

    #[tokio::test]
    async fn commits_pushed_from_outside_are_merged_in_and_the_push_then_lands()
    -> anyhow::Result<()> {
        let s = scratch().await?;
        commit_file(&s.outside, "docs/research.md", "found\n").await?;
        said(&s.outside, &["push", "-q", "origin", "main"]).await?;
        commit_file(&s.fleet, "c.txt", "a seat's commit\n").await?;
        std::fs::write(
            s.fleet.join("b.txt"),
            "another seat is still writing this\n",
        )?;
        let repo = s.fleet.to_string_lossy().into_owned();

        let refused = push_once(&repo, "main").await;
        assert!(
            refused.as_ref().is_err_and(|why| behind_origin(why)),
            "{refused:?}"
        );
        fetch_origin(&repo, "main")
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(merge_origin(&repo, "main").await, Ok(true));
        assert!(s.fleet.join("docs/research.md").is_file());
        assert_eq!(
            std::fs::read_to_string(s.fleet.join("b.txt"))?,
            "another seat is still writing this\n"
        );
        assert_eq!(push_once(&repo, "main").await, Ok(()));
        assert_eq!(merge_origin(&repo, "main").await, Ok(false));
        Ok(())
    }

    #[tokio::test]
    async fn an_outside_commit_over_a_seats_unfinished_file_is_refused_and_nothing_is_touched()
    -> anyhow::Result<()> {
        let s = scratch().await?;
        commit_file(&s.outside, "a.txt", "outside\n").await?;
        said(&s.outside, &["push", "-q", "origin", "main"]).await?;
        commit_file(&s.fleet, "c.txt", "a seat's commit\n").await?;
        std::fs::write(s.fleet.join("a.txt"), "a seat is still writing this\n")?;
        let repo = s.fleet.to_string_lossy().into_owned();

        fetch_origin(&repo, "main")
            .await
            .map_err(anyhow::Error::msg)?;
        let merged = merge_origin(&repo, "main").await;
        assert!(
            merged.as_ref().is_err_and(
                |why| why.starts_with("origin/main holds commits this checkout does not")
            ),
            "{merged:?}"
        );
        assert_eq!(
            std::fs::read_to_string(s.fleet.join("a.txt"))?,
            "a seat is still writing this\n"
        );
        assert!(!s.fleet.join(".git/MERGE_HEAD").exists());
        Ok(())
    }
}
