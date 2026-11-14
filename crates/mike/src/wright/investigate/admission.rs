use std::fs::{File, TryLockError};
use std::path::Path;

pub(crate) fn load_over(loadavg: &str, processors: usize) -> Option<String> {
    let one: f64 = loadavg.split_whitespace().next()?.parse().ok()?;
    let limit = processors as f64;
    (one > limit)
        .then(|| format!("the 1-minute load is {one:.2}, above the {processors} processors"))
}

pub(crate) fn lock_held(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => false,
    }
}

pub(crate) struct Locks<'a> {
    pub(crate) loadavg: &'a Path,
    pub(crate) deploy: &'a Path,
    pub(crate) build: &'a Path,
}

pub(crate) fn machine_busy(locks: &Locks<'_>) -> Option<String> {
    let processors = std::thread::available_parallelism().map_or(1, usize::from);
    if let Ok(loadavg) = std::fs::read_to_string(locks.loadavg)
        && let Some(reason) = load_over(&loadavg, processors)
    {
        return Some(reason);
    }
    if lock_held(locks.deploy) {
        return Some(format!("a deploy holds {}", locks.deploy.display()));
    }
    if lock_held(locks.build) {
        return Some(format!("a heavy build holds {}", locks.build.display()));
    }
    None
}
