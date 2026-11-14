use std::path::Path;
use std::time::Duration;

const GIT_TIMEOUT: Duration = Duration::from_secs(180);
const BASE: &str = "origin/main";

async fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let run = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(GIT_TIMEOUT, run)
        .await
        .map_err(|_| format!("git {} timed out after {GIT_TIMEOUT:?}", args.join(" ")))?
        .map_err(|e| format!("git {} could not start: {e}", args.join(" ")))?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "git {} in {} failed: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

pub(crate) async fn refresh(repo: &Path, slot: &Path) -> Result<(), String> {
    if let Err(e) = git(repo, &["fetch", "--quiet", "origin", "main"]).await {
        tracing::warn!(error = %e, "wright: fetching origin/main failed; the slot uses the last fetched main");
    }
    if !slot.join(".git").exists() {
        let shown = slot.to_string_lossy();
        return git(repo, &["worktree", "add", "--detach", &shown, BASE]).await;
    }
    git(slot, &["checkout", "--detach", "--force", BASE]).await?;
    git(slot, &["clean", "-ffdx", "--quiet"]).await
}
