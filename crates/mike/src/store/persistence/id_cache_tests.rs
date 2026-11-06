use uuid::Uuid;

use super::{Bounded, CachedProject, IdCache};

#[test]
fn bounded_past_capacity_keeps_newest_half() {
    let map: Bounded<u32, u32> = Bounded::new(10);
    for key in 0..11 {
        map.insert(key, key * 2);
    }
    assert!(
        map.entries.len() <= 6,
        "eviction must drop the oldest entries"
    );
    assert_eq!(map.get(&10), Some(20), "the newest entry survives eviction");
    assert_eq!(map.get(&0), None, "the oldest entry is evicted");
}

#[test]
fn bounded_under_capacity_keeps_every_entry() {
    let map: Bounded<u32, u32> = Bounded::new(10);
    for key in 0..10 {
        map.insert(key, key);
    }
    assert_eq!(map.entries.len(), 10);
    assert_eq!(map.get(&0), Some(0));
}

#[test]
fn session_forget_clears_every_session_and_keeps_projects() {
    let cache = IdCache::with_capacity(8, 8);
    let session = Uuid::new_v4();
    cache.remember_session(cache.session_epoch(), session, 7);
    let project = CachedProject {
        id: 3,
        path: "/repo".to_owned(),
        via_suffix: false,
    };
    cache.remember_project("/repo", project.clone());
    cache.forget_sessions();
    assert_eq!(cache.session(&session), None);
    assert_eq!(cache.project("/repo"), Some(project));
}

#[test]
fn project_forget_clears_projects_and_keeps_sessions() {
    let cache = IdCache::with_capacity(8, 8);
    let session = Uuid::new_v4();
    cache.remember_session(cache.session_epoch(), session, 7);
    cache.remember_project(
        "/repo",
        CachedProject {
            id: 3,
            path: "/repo".to_owned(),
            via_suffix: false,
        },
    );
    cache.forget_projects();
    assert_eq!(cache.project("/repo"), None);
    assert_eq!(cache.session(&session), Some(7));
}

#[test]
fn session_read_before_forget_is_not_remembered() {
    let cache = IdCache::with_capacity(8, 8);
    let session = Uuid::new_v4();
    let read_at = cache.session_epoch();
    cache.forget_sessions();
    cache.remember_session(read_at, session, 7);
    assert_eq!(cache.session(&session), None);
}

#[test]
fn clones_share_one_cache() {
    let cache = IdCache::default();
    let clone = cache.clone();
    let session = Uuid::new_v4();
    clone.remember_session(clone.session_epoch(), session, 11);
    assert_eq!(cache.session(&session), Some(11));
}
