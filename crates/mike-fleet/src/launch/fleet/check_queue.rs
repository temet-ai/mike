use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use tokio::sync::{Mutex, OwnedMutexGuard};

pub async fn hold(repo: &Path) -> OwnedMutexGuard<()> {
    static SLOTS: OnceLock<DashMap<PathBuf, Arc<Mutex<()>>>> = OnceLock::new();
    let key = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    let slot = SLOTS
        .get_or_init(DashMap::new)
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    slot.lock_owned().await
}
