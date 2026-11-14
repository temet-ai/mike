use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::definition::FleetDefinition;
use super::report::newest_commit_of;

pub const CHECK_NOT_COVERED: &str = "no check covers these paths";

pub fn check_paths(def: &FleetDefinition) -> Vec<String> {
    def.execution
        .operation(&def.check.command)
        .filter(|op| !op.inputs.is_empty())
        .map_or_else(|| def.check.covered(), |op| op.inputs.clone())
}

pub fn outside_check(changed: &[String], covered: &[String]) -> bool {
    !covered.is_empty()
        && !changed.is_empty()
        && !changed
            .iter()
            .any(|path| super::commit_paths::covers(covered, path))
}

pub async fn changed_files(repo: &str, sha: &str) -> Vec<String> {
    git(
        repo,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "--root",
            sha,
        ],
    )
    .await
    .map(|out| out.lines().map(str::to_string).collect())
    .unwrap_or_default()
}

pub fn judged_commit_of(
    commits: &[(Value, DateTime<Utc>)],
    reports: &[(Value, DateTime<Utc>)],
    task: &str,
) -> Option<String> {
    let reported = reports.iter().filter_map(|(data, at)| {
        let sha = data.get("commit").and_then(Value::as_str)?;
        let task = data.get("task").and_then(Value::as_str)?;
        Some((json!({"task": task, "sha": sha}), *at))
    });
    let all: Vec<_> = commits.iter().cloned().chain(reported).collect();
    newest_commit_of(&all, task)
}

pub async fn named_commit(
    repo: &str,
    branch: &str,
    own: Option<&str>,
    named: &str,
) -> Result<String, String> {
    let sha = git(
        repo,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", named.trim()),
        ],
    )
    .await
    .ok_or_else(|| format!("`{named}` is not a commit in this repository"))?;
    if let Some(own) = own
        && !ancestor(repo, own, &sha).await
    {
        return Err(format!(
            "`{named}` does not contain this task's last commit {own}; name a commit on {branch} that does, or leave sha out"
        ));
    }
    let tip = if branch.trim().is_empty() {
        "HEAD".to_string()
    } else {
        format!("refs/heads/{}", branch.trim())
    };
    if !ancestor(repo, &sha, &tip).await {
        return Err(format!("`{named}` is not on {tip}"));
    }
    Ok(sha)
}

pub async fn ancestor(repo: &str, older: &str, newer: &str) -> bool {
    tokio::process::Command::new("git")
        .args(["-C", repo, "merge-base", "--is-ancestor", older, newer])
        .output()
        .await
        .is_ok_and(|out| out.status.success())
}

async fn git(repo: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|sha| !sha.is_empty())
}

#[cfg(test)]
#[path = "report_commit_tests.rs"]
mod tests;
