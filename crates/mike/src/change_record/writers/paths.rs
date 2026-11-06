use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::process::Command;

use super::error::WriterError;
use super::words::{commit_candidates, path_candidates};

const TRACKED_TTL: Duration = Duration::from_secs(60);

type TrackedCache = Mutex<HashMap<String, (Instant, Arc<HashSet<String>>)>>;

fn cache() -> &'static TrackedCache {
    static CACHE: OnceLock<TrackedCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn git(checkout: &Path, args: &[&str]) -> Result<Vec<u8>, WriterError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .await
        .map_err(|e| WriterError::Git {
            checkout: checkout.to_path_buf(),
            command: args.join(" "),
            message: e.to_string(),
        })?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    Err(WriterError::Git {
        checkout: checkout.to_path_buf(),
        command: args.join(" "),
        message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

async fn ensure_repository(checkout: &Path) -> Result<(), WriterError> {
    if !checkout.is_dir() {
        return Err(WriterError::NotARepository(checkout.to_path_buf()));
    }
    git(checkout, &["rev-parse", "--git-dir"])
        .await
        .map(|_| ())
        .map_err(|_| WriterError::NotARepository(checkout.to_path_buf()))
}

fn split_nul(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
}

async fn tracked(checkout: &Path) -> Result<Arc<HashSet<String>>, WriterError> {
    let key = checkout.to_string_lossy().into_owned();
    if let Ok(held) = cache().lock()
        && let Some((at, files)) = held.get(&key)
        && at.elapsed() < TRACKED_TTL
    {
        return Ok(files.clone());
    }
    let listing = git(checkout, &["ls-files", "-z"]).await?;
    let files: Arc<HashSet<String>> = Arc::new(split_nul(&listing).collect());
    if let Ok(mut held) = cache().lock() {
        held.insert(key, (Instant::now(), files.clone()));
    }
    Ok(files)
}

pub(super) async fn commit_paths(checkout: &Path, commit: &str) -> Vec<String> {
    match git(
        checkout,
        &[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            commit,
        ],
    )
    .await
    {
        Ok(listing) => split_nul(&listing).collect(),
        Err(_) => Vec::new(),
    }
}

fn push_new(into: &mut Vec<String>, path: String) {
    if !into.contains(&path) {
        into.push(path);
    }
}

pub async fn paths_named(text: &str, checkout: &Path) -> Result<Vec<String>, WriterError> {
    ensure_repository(checkout).await?;
    let mut found: Vec<String> = Vec::new();
    for commit in commit_candidates(text) {
        for path in commit_paths(checkout, &commit).await {
            push_new(&mut found, path);
        }
    }
    let files = tracked(checkout).await?;
    for word in path_candidates(text, &checkout.to_string_lossy()) {
        if files.contains(&word) {
            push_new(&mut found, word);
        }
    }
    Ok(found)
}
