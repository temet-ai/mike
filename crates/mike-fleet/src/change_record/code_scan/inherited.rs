use std::collections::HashSet;
use std::time::Duration;

use mike_code::scan;

use super::project_rules::ActiveRules;
use crate::change_record::watch::Git;

const VERSIONS: &str = "20";
const GIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum InheritedError {
    #[error(transparent)]
    Git(#[from] crate::change_record::watch::git::GitError),
    #[error("git took longer than {GIT_TIMEOUT:?}")]
    Timeout,
    #[error("the scan of an earlier version stopped: {0}")]
    Scan(#[from] tokio::task::JoinError),
}

pub async fn inherited(
    git: &Git,
    rules: ActiveRules,
    path: &str,
) -> Result<HashSet<String>, InheritedError> {
    let versions = tokio::time::timeout(GIT_TIMEOUT, foreign_versions(git, path))
        .await
        .map_err(|_| InheritedError::Timeout)??;
    let path = path.to_string();
    Ok(tokio::task::spawn_blocking(move || {
        versions
            .iter()
            .flat_map(|bytes| scan(rules.get(), &path, bytes).findings)
            .map(|finding| finding.fingerprint)
            .collect()
    })
    .await?)
}

async fn foreign_versions(
    git: &Git,
    path: &str,
) -> Result<Vec<Vec<u8>>, crate::change_record::watch::git::GitError> {
    let reflog = git
        .run_with(
            &["reflog", "show", "--no-abbrev", "--format=%H %gs", "HEAD"],
            None,
            &[0, 128],
        )
        .await?;
    let own = committed_here(&String::from_utf8_lossy(&reflog));
    let log = git
        .run(&[
            "log",
            "--all",
            "-n",
            VERSIONS,
            "--no-abbrev",
            "--raw",
            "--no-renames",
            "--format=commit %H",
            "--",
            path,
        ])
        .await?;
    let mut versions = Vec::new();
    for sha in foreign_blobs(&String::from_utf8_lossy(&log), &own) {
        versions.push(git.run(&["cat-file", "blob", &sha]).await?);
    }
    Ok(versions)
}

pub fn committed_here(reflog: &str) -> HashSet<String> {
    reflog
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(_, action)| action.starts_with("commit"))
        .map(|(sha, _)| sha.to_string())
        .collect()
}

pub fn foreign_blobs(log: &str, own: &HashSet<String>) -> Vec<String> {
    let mut foreign = false;
    let mut blobs = Vec::new();
    for line in log.lines() {
        if let Some(sha) = line.strip_prefix("commit ") {
            foreign = !own.contains(sha);
        } else if foreign && let Some(raw) = line.strip_prefix(':') {
            let mut fields = raw.split_whitespace();
            let (mode, blob) = (fields.nth(1), fields.nth(1));
            if let (Some("100644" | "100755"), Some(blob)) = (mode, blob)
                && !blobs.iter().any(|known| known == blob)
            {
                blobs.push(blob.to_string());
            }
        }
    }
    blobs
}
