use std::collections::HashMap;
use std::sync::Arc;

use super::*;

fn snapshot() -> KeeperSnapshot {
    let mut projects = HashMap::new();
    projects.insert(1, Arc::new(ProjectThreads::default()));
    projects.insert(2, Arc::new(ProjectThreads::default()));
    KeeperSnapshot {
        generation: 1,
        projects,
        roots: vec![("/work".to_owned(), 1), ("/work/app/".to_owned(), 2)],
    }
}

fn resolved(snapshot: &KeeperSnapshot, path: &str) -> Option<*const ProjectThreads> {
    snapshot.project_for(path).map(std::ptr::from_ref)
}

#[test]
fn project_for_nested_roots_picks_the_longest_match() {
    let snap = snapshot();
    let app = snap.projects.get(&2).map(|p| Arc::as_ptr(p));
    assert_eq!(resolved(&snap, "/work/app"), app);
    assert_eq!(resolved(&snap, "/work/app/crates/mike"), app);
}

#[test]
fn project_for_sibling_with_shared_prefix_does_not_match_the_root() {
    let snap = snapshot();
    let outer = snap.projects.get(&1).map(|p| Arc::as_ptr(p));
    assert_eq!(resolved(&snap, "/work/application"), outer);
}

#[test]
fn project_for_unknown_path_is_none() {
    assert!(snapshot().project_for("/elsewhere").is_none());
}
