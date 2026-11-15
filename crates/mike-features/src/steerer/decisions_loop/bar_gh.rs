use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

pub(crate) const REPO: &str = "temet-ai/mike";
pub(crate) const MILESTONE: &str = "Jev decisions";
pub(crate) const LABEL: &str = "backend";
const GH_TIMEOUT: Duration = Duration::from_secs(30);

#[async_trait]
pub(crate) trait IssueTracker: Send + Sync {
    async fn open_issue(&self, title: &str) -> Result<Option<u32>, String>;
    async fn milestone_open(&self, name: &str) -> Result<bool, String>;
    async fn create(&self, title: &str, body: &str, milestone: Option<&str>)
    -> Result<u32, String>;
    async fn comment(&self, issue: u32, body: &str) -> Result<(), String>;
}

pub(crate) struct Gh;

#[derive(Deserialize)]
struct Listed {
    number: u32,
    title: String,
}

async fn run_gh(args: &[&str]) -> Result<String, String> {
    let mut command = tokio::process::Command::new("gh");
    command.args(args).stdin(Stdio::null()).kill_on_drop(true);
    let name = args.iter().take(2).copied().collect::<Vec<_>>().join(" ");
    let output = tokio::time::timeout(GH_TIMEOUT, command.output())
        .await
        .map_err(|_| format!("gh {name} timed out after {} s", GH_TIMEOUT.as_secs()))?
        .map_err(|err| format!("could not run gh {name}: {err}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    Err(format!(
        "gh {name} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn issue_number(stdout: &str) -> Result<u32, String> {
    stdout
        .lines()
        .last()
        .and_then(|line| line.trim().rsplit('/').next())
        .and_then(|tail| tail.parse().ok())
        .ok_or_else(|| format!("gh issue create printed no issue number: {stdout:?}"))
}

#[async_trait]
impl IssueTracker for Gh {
    async fn open_issue(&self, title: &str) -> Result<Option<u32>, String> {
        let search = format!("{title} in:title");
        let out = run_gh(&[
            "issue",
            "list",
            "--repo",
            REPO,
            "--state",
            "open",
            "--search",
            &search,
            "--json",
            "number,title",
            "--limit",
            "100",
        ])
        .await?;
        let listed: Vec<Listed> = serde_json::from_str(&out)
            .map_err(|err| format!("gh issue list gave unreadable JSON ({err}): {out:?}"))?;
        Ok(listed
            .into_iter()
            .find(|issue| issue.title == title)
            .map(|issue| issue.number))
    }

    async fn milestone_open(&self, name: &str) -> Result<bool, String> {
        let path = format!("repos/{REPO}/milestones?state=open&per_page=100");
        let out = run_gh(&["api", &path, "--jq", ".[].title"]).await?;
        Ok(out.lines().any(|line| line.trim() == name))
    }

    async fn create(
        &self,
        title: &str,
        body: &str,
        milestone: Option<&str>,
    ) -> Result<u32, String> {
        let mut args = vec![
            "issue", "create", "--repo", REPO, "--title", title, "--body", body, "--label", LABEL,
        ];
        if let Some(name) = milestone {
            args.extend(["--milestone", name]);
        }
        issue_number(&run_gh(&args).await?)
    }

    async fn comment(&self, issue: u32, body: &str) -> Result<(), String> {
        let number = issue.to_string();
        run_gh(&["issue", "comment", &number, "--repo", REPO, "--body", body])
            .await
            .map(|_| ())
    }
}
