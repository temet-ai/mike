use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;

use super::resolve::ThreadFacts;

pub type KeeperCache = Arc<ArcSwap<KeeperSnapshot>>;

#[derive(Debug, Clone)]
pub struct ThreadEntry {
    pub thread_id: i64,
    pub facts: ThreadFacts,
    pub text: Arc<str>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectThreads {
    pub threads: Vec<ThreadEntry>,
}

impl ProjectThreads {
    pub fn entry(&self, slug: &str) -> Option<&ThreadEntry> {
        self.threads.iter().find(|entry| entry.facts.slug == slug)
    }

    pub fn facts(&self) -> Vec<ThreadFacts> {
        self.threads
            .iter()
            .map(|entry| entry.facts.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct KeeperSnapshot {
    pub generation: i64,
    pub projects: HashMap<i64, Arc<ProjectThreads>>,
    pub roots: Vec<(String, i64)>,
}

impl KeeperSnapshot {
    pub fn project_for(&self, path: &str) -> Option<&ProjectThreads> {
        let path = path.trim_end_matches('/');
        self.roots
            .iter()
            .filter(|(root, _)| under_root(path, root.trim_end_matches('/')))
            .max_by_key(|(root, _)| root.len())
            .and_then(|(_, project)| self.projects.get(project))
            .map(Arc::as_ref)
    }
}

fn under_root(path: &str, root: &str) -> bool {
    !root.is_empty()
        && (path == root
            || path
                .strip_prefix(root)
                .is_some_and(|rest| rest.starts_with('/')))
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
pub mod tests;
