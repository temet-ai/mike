use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use mike_code::SourceFile;
use mike_code::wiring::MAX_WIRING_BYTES;

const TEXT_EXTENSIONS: [&str; 9] = ["rs", "ts", "tsx", "js", "mjs", "svelte", "py", "sh", "bash"];
const SKIPPED_PARTS: [&str; 4] = [
    "node_modules/",
    "/target/",
    ".svelte-kit/",
    ".claude/worktrees/",
];

pub(super) fn repo_root(start: &Path) -> Result<std::path::PathBuf> {
    let out = git_text(start, &["rev-parse", "--show-toplevel"])?;
    Ok(std::path::PathBuf::from(out.trim()))
}

pub(super) fn working_tree(root: &Path) -> Result<Vec<SourceFile>> {
    let listing = git_bytes(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    let mut files = Vec::new();
    for path in paths_of(&listing) {
        if let Ok(content) = std::fs::read_to_string(root.join(&path))
            && content.len() <= MAX_WIRING_BYTES
        {
            files.push(SourceFile { path, content });
        }
    }
    Ok(files)
}

pub(super) fn at_revision(root: &Path, revision: &str) -> Result<Vec<SourceFile>> {
    let listing = git_bytes(root, &["ls-tree", "-r", "-z", "--name-only", revision])?;
    let paths = paths_of(&listing);
    read_blobs(root, revision, &paths)
}

pub(super) fn is_commit(root: &Path, revision: &str) -> bool {
    let spec = format!("{revision}^{{commit}}");
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "-e", spec.as_str()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(super) fn merge_base(root: &Path, left: &str, right: &str) -> Result<String> {
    Ok(git_text(root, &["merge-base", left, right])?
        .trim()
        .to_owned())
}

fn paths_of(listing: &[u8]) -> Vec<String> {
    listing
        .split(|byte| *byte == 0)
        .filter_map(|raw| std::str::from_utf8(raw).ok())
        .filter(|path| is_wanted(path))
        .map(str::to_owned)
        .collect()
}

fn is_wanted(path: &str) -> bool {
    if SKIPPED_PARTS.iter().any(|part| path.contains(part)) {
        return false;
    }
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some(extension) => TEXT_EXTENSIONS.contains(&extension),
        None => path.starts_with("scripts/"),
    }
}

fn read_blobs(root: &Path, revision: &str, paths: &[String]) -> Result<Vec<SourceFile>> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start git cat-file --batch")?;
    let mut stdin = child.stdin.take().context("git cat-file has no stdin")?;
    let stdout = child.stdout.take().context("git cat-file has no stdout")?;
    let requests: Vec<String> = paths
        .iter()
        .map(|path| format!("{revision}:{path}\n"))
        .collect();
    let writer = std::thread::spawn(move || {
        for request in requests {
            if stdin.write_all(request.as_bytes()).is_err() {
                break;
            }
        }
    });
    let files = parse_batch(BufReader::new(stdout), paths)?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("the git cat-file writer thread panicked"))?;
    child.wait().context("git cat-file did not exit")?;
    Ok(files)
}

fn parse_batch(mut reader: impl BufRead, paths: &[String]) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    let mut header = String::new();
    for path in paths {
        header.clear();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        let fields: Vec<&str> = header.split_whitespace().collect();
        let [_, kind, size] = fields.as_slice() else {
            continue;
        };
        let Ok(size) = size.parse::<usize>() else {
            continue;
        };
        let mut body = vec![0u8; size];
        reader
            .read_exact(&mut body)
            .with_context(|| format!("short read of {path}"))?;
        let mut newline = [0u8; 1];
        reader.read_exact(&mut newline)?;
        if *kind == "blob"
            && size <= MAX_WIRING_BYTES
            && let Ok(content) = String::from_utf8(body)
        {
            files.push(SourceFile {
                path: path.clone(),
                content,
            });
        }
    }
    Ok(files)
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .with_context(|| format!("cannot run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&git_bytes(root, args)?).into_owned())
}
