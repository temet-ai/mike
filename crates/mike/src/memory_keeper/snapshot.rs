use std::collections::HashMap;
use std::sync::Arc;

use super::resolve::ThreadFacts;

#[derive(Debug, Clone)]
pub(crate) struct ThreadEntry {
    pub(crate) facts: ThreadFacts,
    pub(crate) text: Arc<str>,
    pub(crate) content_hash: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProjectThreads {
    pub(crate) threads: Vec<ThreadEntry>,
}

impl ProjectThreads {
    pub(crate) fn entry(&self, slug: &str) -> Option<&ThreadEntry> {
        self.threads.iter().find(|entry| entry.facts.slug == slug)
    }

    pub(crate) fn facts(&self) -> Vec<ThreadFacts> {
        self.threads
            .iter()
            .map(|entry| entry.facts.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct KeeperSnapshot {
    pub(crate) generation: i64,
    pub(crate) projects: HashMap<i64, Arc<ProjectThreads>>,
    pub(crate) roots: Vec<(String, i64)>,
}

impl KeeperSnapshot {
    pub(crate) fn project_for(&self, path: &str) -> Option<&ProjectThreads> {
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
mod tests;
