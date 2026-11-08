use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

pub(crate) const QUEUE: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Path(String),
    Refs,
    Rescan,
}

#[derive(Debug, Clone)]
pub struct Places {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

pub struct Notifier {
    watcher: RecommendedWatcher,
    root: PathBuf,
}

impl Notifier {
    pub fn start(
        places: &Places,
        dirs: impl IntoIterator<Item = String>,
        tx: mpsc::Sender<Signal>,
        overflow: Arc<AtomicBool>,
    ) -> notify::Result<Self> {
        let route = places.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            for signal in signals(&route, event) {
                if tx.try_send(signal).is_err() {
                    overflow.store(true, Ordering::Release);
                }
            }
        })?;
        watcher.watch(&places.root, RecursiveMode::NonRecursive)?;
        watcher.watch(&places.common_dir, RecursiveMode::NonRecursive)?;
        watcher.watch(&places.common_dir.join("refs"), RecursiveMode::Recursive)?;
        if places.git_dir != places.common_dir {
            watcher.watch(&places.git_dir, RecursiveMode::NonRecursive)?;
        }
        let mut notifier = Self {
            watcher,
            root: places.root.clone(),
        };
        notifier.add_dirs(dirs);
        Ok(notifier)
    }

    pub fn add_dirs(&mut self, dirs: impl IntoIterator<Item = String>) {
        for dir in dirs {
            let path = self.root.join(&dir);
            if let Err(err) = self.watcher.watch(&path, RecursiveMode::NonRecursive) {
                tracing::debug!(dir = %dir, error = %err, "change record: a folder could not be watched");
            }
        }
    }
}

pub fn signals(places: &Places, event: notify::Result<Event>) -> Vec<Signal> {
    let Ok(event) = event else {
        return vec![Signal::Rescan];
    };
    if event.need_rescan() {
        return vec![Signal::Rescan];
    }
    if is_read_only(&event.kind) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for path in &event.paths {
        if let Some(signal) = classify(places, path)
            && !out.contains(&signal)
        {
            out.push(signal);
        }
    }
    out
}

fn is_read_only(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => false,
        EventKind::Access(_) => true,
        _ => false,
    }
}

pub fn classify(places: &Places, path: &Path) -> Option<Signal> {
    for git in [&places.git_dir, &places.common_dir] {
        if let Ok(inside) = path.strip_prefix(git) {
            return is_ref_path(inside).then_some(Signal::Refs);
        }
    }
    let relative = path.strip_prefix(&places.root).ok()?;
    let text = relative.to_str()?;
    if text.is_empty() || text == ".git" || text.starts_with(".git/") {
        return None;
    }
    Some(Signal::Path(text.to_string()))
}

fn is_ref_path(inside: &Path) -> bool {
    let first = inside.components().next();
    match first.and_then(|c| c.as_os_str().to_str()) {
        Some("refs") => true,
        Some(name) => inside.components().count() == 1 && matches!(name, "HEAD" | "packed-refs"),
        None => false,
    }
}
