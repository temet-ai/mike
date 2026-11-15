use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub(super) type Manifest = BTreeMap<String, String>;
pub(super) type Conflicts = BTreeMap<String, String>;

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn atomic_write(path: &Path, body: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".mike-")
        .tempfile_in(parent)?;
    staged.write_all(body.as_bytes())?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn conflict_message(path: &Path) -> String {
    format!(
        "Generated memory/skill was edited or is not owned: {}; preserve and reconcile it",
        path.display()
    )
}

pub(super) enum Owned {
    Written,
    Adopted,
    Conflict(String),
}

pub(super) fn write_owned(
    path: &Path,
    body: &str,
    manifest: &mut Manifest,
    apply: bool,
) -> std::io::Result<Owned> {
    let key = path.display().to_string();
    let digest = sha256_hex(body.as_bytes());
    if path.exists() {
        let existing = fs::read(path)?;
        let owned = manifest
            .get(&key)
            .is_some_and(|known| *known == sha256_hex(&existing));
        if !owned {
            if existing == body.as_bytes() {
                manifest.insert(key, digest);
                return Ok(Owned::Adopted);
            }
            return Ok(Owned::Conflict(conflict_message(path)));
        }
    }
    if apply {
        atomic_write(path, body)?;
    }
    manifest.insert(key, digest);
    Ok(Owned::Written)
}

fn remove_emptied_directories(path: &Path) {
    for directory in [path.parent(), path.parent().and_then(Path::parent)]
        .into_iter()
        .flatten()
    {
        if fs::remove_dir(directory).is_err() {
            break;
        }
    }
}

fn retire(old: &str, recorded: &str, state: &Path, apply: bool) -> std::io::Result<Option<String>> {
    let path = Path::new(old);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    if sha256_hex(&bytes) != recorded {
        return Ok(Some(format!(
            "Preserved manually edited retired file: {}",
            path.display()
        )));
    }
    if apply {
        let archive = state
            .join("retired")
            .join(format!("{}.md", sha256_hex(old.as_bytes())));
        atomic_write(&archive, &String::from_utf8_lossy(&bytes))?;
        fs::remove_file(path)?;
        remove_emptied_directories(path);
    }
    Ok(None)
}

pub(super) fn reconcile(
    desired: &BTreeMap<String, String>,
    manifest: &mut Manifest,
    state: &Path,
    apply: bool,
) -> std::io::Result<Conflicts> {
    let mut conflicts = Conflicts::new();
    for (path, body) in desired {
        if let Owned::Conflict(message) = write_owned(Path::new(path), body, manifest, apply)? {
            conflicts.insert(path.clone(), message);
        }
    }
    let retired: Vec<(String, String)> = manifest
        .iter()
        .filter(|(old, _)| !desired.contains_key(*old))
        .map(|(old, recorded)| (old.clone(), recorded.clone()))
        .collect();
    for (old, recorded) in retired {
        match retire(&old, &recorded, state, apply)? {
            Some(message) => {
                conflicts.insert(old, message);
            }
            None => {
                manifest.remove(&old);
            }
        }
    }
    Ok(conflicts)
}

pub(super) fn exclude_file(checkout: &Path) -> Option<PathBuf> {
    let git = checkout.join(".git");
    if git.is_dir() {
        return Some(git.join("info/exclude"));
    }
    let text = fs::read_to_string(&git).ok()?;
    let pointer = text.trim().strip_prefix("gitdir:")?.trim();
    let gitdir = checkout.join(pointer).canonicalize().ok()?;
    let common = match fs::read_to_string(gitdir.join("commondir")) {
        Ok(recorded) => gitdir.join(recorded.trim()).canonicalize().ok()?,
        Err(_) => gitdir,
    };
    Some(common.join("info/exclude"))
}

pub(super) fn add_exclude_lines(path: &Path, lines: &[String]) -> std::io::Result<()> {
    let mut text = fs::read_to_string(path).unwrap_or_default();
    let have: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut missing: Vec<&String> = Vec::new();
    for line in lines {
        if !have.contains(line) && !missing.contains(&line) {
            missing.push(line);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for line in missing {
        text.push_str(line);
        text.push('\n');
    }
    atomic_write(path, &text)
}

#[cfg(test)]
#[path = "owned_tests.rs"]
mod owned_tests;
