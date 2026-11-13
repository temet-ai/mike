use std::path::{Component, Path};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub const BYTE_LIMIT: usize = 2 * 1024 * 1024;

pub fn valid_object(value: &str) -> bool {
    (4..=64).contains(&value.len()) && value.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn relative_path(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || Path::new(value)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || value.contains('\0')
    {
        return Err("path must be a nonempty checkout-relative path without traversal".into());
    }
    Ok(value)
}

pub async fn run(repo: &str, args: &[&str], differences: bool) -> Result<(Vec<u8>, bool), String> {
    tokio::time::timeout(Duration::from_secs(10), output(repo, args, differences))
        .await
        .map_err(|_| "diff lookup exceeded ten seconds".to_string())?
}

async fn output(repo: &str, args: &[&str], differences: bool) -> Result<(Vec<u8>, bool), String> {
    let mut child = Command::new("git")
        .arg("--no-pager")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("git has no output stream")?;
    let mut bytes = Vec::new();
    stdout
        .take((BYTE_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    let truncated = bytes.len() > BYTE_LIMIT;
    if truncated {
        bytes.truncate(BYTE_LIMIT);
        let _ = child.kill().await;
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !(truncated || status.success() || differences && status.code() == Some(1)) {
        return Err(format!("git lookup failed with status {:?}", status.code()));
    }
    Ok((bytes, truncated))
}

pub async fn resolve_commit(repo: &str, value: &str) -> Result<String, String> {
    if !valid_object(value) {
        return Err("commit must be four to 64 hexadecimal digits".into());
    }
    let revision = format!("{value}^{{commit}}");
    let (bytes, _) = run(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &revision],
        false,
    )
    .await?;
    let sha = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    Ok(sha.trim().to_string())
}

pub fn text(bytes: &[u8], limit: usize) -> Result<(String, bool), String> {
    let source =
        std::str::from_utf8(bytes).map_err(|_| "diff is not UTF-8; no lossy text returned")?;
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    Ok((
        lines.iter().take(limit).copied().collect(),
        lines.len() > limit,
    ))
}
