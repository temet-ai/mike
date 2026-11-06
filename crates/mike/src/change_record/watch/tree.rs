use std::collections::{BTreeMap, BTreeSet};

use super::git::{Git, GitError};
use crate::launch::fleet::watch::{Seen, parse_index, parse_status};

const STATUS: [&str; 4] = ["status", "--porcelain=v2", "-z", "--untracked-files=all"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaKind {
    Added,
    Modified,
    Deleted,
}

impl DeltaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DeltaKind::Added => "added",
            DeltaKind::Modified => "modified",
            DeltaKind::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDelta {
    pub path: String,
    pub change: DeltaKind,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
}

#[derive(Debug, Default)]
pub struct Observed {
    pub deltas: Vec<FileDelta>,
    pub new_dirs: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct KnownTree {
    known: BTreeMap<String, String>,
}

enum OnDisk {
    File,
    Dir,
    Gone,
    Other,
}

impl KnownTree {
    pub async fn baseline(git: &Git) -> Result<Self, GitError> {
        Ok(Self {
            known: current(git).await?,
        })
    }

    pub fn directories(&self) -> BTreeSet<String> {
        let mut dirs = BTreeSet::new();
        for path in self.known.keys() {
            dirs.extend(parents(path));
        }
        dirs
    }

    pub async fn observe_paths(
        &mut self,
        git: &Git,
        paths: &BTreeSet<String>,
    ) -> Result<Observed, GitError> {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let mut gone = Vec::new();
        for path in paths {
            match on_disk(git, path) {
                OnDisk::File => files.push(path.clone()),
                OnDisk::Dir => dirs.push(path.clone()),
                OnDisk::Gone => gone.push(path.clone()),
                OnDisk::Other => {}
            }
        }
        let mut observed = Observed::default();
        for path in &gone {
            observed.deltas.extend(self.forget(path));
        }
        let mut wanted: Vec<String> = files.iter().chain(dirs.iter()).cloned().collect();
        let ignored: BTreeSet<String> = git.ignored(&wanted).await?.into_iter().collect();
        wanted.retain(|p| !ignored.contains(p));
        let mut to_hash: BTreeSet<String> = BTreeSet::new();
        for path in wanted {
            if dirs.contains(&path) {
                let inside = git.files_under(&path).await?;
                observed.new_dirs.insert(path.clone());
                for file in inside {
                    observed
                        .new_dirs
                        .extend(parents(&file).filter(|d| d.starts_with(&path)));
                    to_hash.insert(file);
                }
            } else {
                to_hash.insert(path);
            }
        }
        let to_hash: Vec<String> = to_hash.into_iter().collect();
        for (path, sha) in git.hash_paths(&to_hash).await? {
            observed.deltas.extend(self.set(path, sha));
        }
        Ok(observed)
    }

    pub async fn backstop(&mut self, git: &Git) -> Result<Vec<FileDelta>, GitError> {
        let now = current(git).await?;
        let deltas = differences(&self.known, &now);
        self.known = now;
        Ok(deltas)
    }

    fn set(&mut self, path: String, sha: String) -> Option<FileDelta> {
        let before = self.known.insert(path.clone(), sha.clone());
        let change = match &before {
            None => DeltaKind::Added,
            Some(old) if *old != sha => DeltaKind::Modified,
            Some(_) => return None,
        };
        Some(FileDelta {
            path,
            change,
            before_sha: before,
            after_sha: Some(sha),
        })
    }

    fn forget(&mut self, path: &str) -> Vec<FileDelta> {
        let prefix = format!("{path}/");
        let doomed: Vec<String> = self
            .known
            .keys()
            .filter(|k| k.as_str() == path || k.starts_with(&prefix))
            .cloned()
            .collect();
        doomed
            .into_iter()
            .filter_map(|k| {
                self.known.remove(&k).map(|old| FileDelta {
                    path: k,
                    change: DeltaKind::Deleted,
                    before_sha: Some(old),
                    after_sha: None,
                })
            })
            .collect()
    }
}

fn on_disk(git: &Git, path: &str) -> OnDisk {
    match std::fs::symlink_metadata(git.absolute(path)) {
        Ok(meta) if meta.file_type().is_file() => OnDisk::File,
        Ok(meta) if meta.file_type().is_dir() => OnDisk::Dir,
        Ok(_) => OnDisk::Other,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => OnDisk::Gone,
        Err(_) => OnDisk::Other,
    }
}

pub fn parents(path: &str) -> impl Iterator<Item = String> + '_ {
    path.match_indices('/')
        .map(move |(i, _)| path[..i].to_string())
}

async fn current(git: &Git) -> Result<BTreeMap<String, String>, GitError> {
    let status = git.run(&STATUS).await?;
    let index = git.run(&["ls-files", "-s", "-z"]).await?;
    let mut now = parse_index(&index);
    let seen = parse_status(&status);
    let mut present = Vec::new();
    for item in seen {
        match item {
            Seen::Gone(path) => {
                now.remove(&path);
            }
            Seen::Present(path) => present.push(path),
        }
    }
    present.retain(|p| matches!(on_disk(git, p), OnDisk::File));
    let hashed = git.hash_paths(&present).await?;
    let hashed_paths: BTreeSet<&String> = hashed.iter().map(|(p, _)| p).collect();
    let missing: Vec<String> = present
        .iter()
        .filter(|p| !hashed_paths.contains(p))
        .cloned()
        .collect();
    for path in missing {
        now.remove(&path);
    }
    for (path, sha) in hashed {
        now.insert(path, sha);
    }
    Ok(now)
}

pub fn differences(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<FileDelta> {
    let mut out = Vec::new();
    for (path, sha) in after {
        let change = match before.get(path) {
            None => DeltaKind::Added,
            Some(old) if old != sha => DeltaKind::Modified,
            Some(_) => continue,
        };
        out.push(FileDelta {
            path: path.clone(),
            change,
            before_sha: before.get(path).cloned(),
            after_sha: Some(sha.clone()),
        });
    }
    for (path, old) in before.iter().filter(|(p, _)| !after.contains_key(*p)) {
        out.push(FileDelta {
            path: path.clone(),
            change: DeltaKind::Deleted,
            before_sha: Some(old.clone()),
            after_sha: None,
        });
    }
    out
}
