use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, SystemTime};

use super::git::{Git, GitError};

const BUILD_DIRS: [&str; 7] = [
    "target",
    "node_modules",
    "dist",
    ".target*",
    "dist-out",
    "__pycache__",
    ".venv",
];

const RACY: Duration = Duration::from_secs(2);

pub fn build_excludes() -> Vec<String> {
    BUILD_DIRS
        .iter()
        .map(|dir| format!(":(exclude,glob)**/{dir}/**"))
        .collect()
}

pub fn is_build_output(path: &str) -> bool {
    path.split('/').any(|part| {
        BUILD_DIRS.iter().any(|dir| match dir.strip_suffix('*') {
            Some(prefix) => part.starts_with(prefix),
            None => part == *dir,
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
}

#[derive(Debug, Default)]
pub struct StatCache {
    seen: HashMap<String, (Stamp, String)>,
}

impl StatCache {
    pub async fn hash(
        &mut self,
        git: &Git,
        paths: &[String],
    ) -> Result<Vec<(String, String)>, GitError> {
        let mut out = Vec::with_capacity(paths.len());
        let mut fresh = Vec::new();
        let mut stamps = HashMap::new();
        for path in paths {
            let stamp = stamp_of(git, path);
            match (stamp, self.seen.get(path)) {
                (Some(now), Some((old, sha))) if *old == now => {
                    out.push((path.clone(), sha.clone()))
                }
                _ => {
                    if let Some(now) = stamp.filter(settled) {
                        stamps.insert(path.clone(), now);
                    }
                    fresh.push(path.clone());
                }
            }
        }
        let hashed = git.hash_paths(&fresh).await?;
        let present: BTreeSet<&String> = paths.iter().collect();
        self.seen.retain(|path, _| present.contains(path));
        for (path, sha) in &hashed {
            match stamps.get(path) {
                Some(stamp) => {
                    self.seen.insert(path.clone(), (*stamp, sha.clone()));
                }
                None => {
                    self.seen.remove(path);
                }
            }
        }
        out.extend(hashed);
        Ok(out)
    }
}

fn stamp_of(git: &Git, path: &str) -> Option<Stamp> {
    let meta = std::fs::symlink_metadata(git.absolute(path)).ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    Some(Stamp {
        len: meta.len(),
        modified: meta.modified().ok()?,
    })
}

fn settled(stamp: &Stamp) -> bool {
    SystemTime::now()
        .duration_since(stamp.modified)
        .is_ok_and(|age| age >= RACY)
}
