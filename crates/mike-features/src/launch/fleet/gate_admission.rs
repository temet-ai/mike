use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use tokio::sync::{Mutex, OwnedMutexGuard};

pub fn key(repo: &Path) -> PathBuf {
    std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf())
}

pub async fn hold(repo: &Path) -> OwnedMutexGuard<()> {
    static GATES: OnceLock<DashMap<PathBuf, Arc<Mutex<()>>>> = OnceLock::new();
    let key = key(repo);
    let gate = GATES
        .get_or_init(DashMap::new)
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    gate.lock_owned().await
}
