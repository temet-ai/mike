use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::*;

fn row(project_id: i64, slug: &str) -> OpenCardRow {
    OpenCardRow {
        thread_id: project_id * 100,
        project_id,
        slug: slug.to_owned(),
        title: format!("{slug} title"),
        goal: String::new(),
        status: "implementing".to_owned(),
        branch: None,
        worktree: None,
        last_session_uuid: None,
        last_checkpoint_at: DateTime::<Utc>::UNIX_EPOCH,
        text: format!("card {slug}"),
        content_hash: format!("hash-{slug}"),
        linked_sessions: vec![Uuid::from_u128(1), Uuid::from_u128(1)],
        checkout_dirs: vec!["/work/app".to_owned()],
    }
}

fn root(path: &str, project_id: i64) -> LocationRoot {
    LocationRoot {
        root_path: path.to_owned(),
        project_id,
    }
}

#[test]
fn build_rows_group_by_project_and_roots_resolve_to_them() {
    let snapshot = build(
        4,
        vec![row(1, "alpha"), row(1, "beta"), row(2, "gamma")],
        vec![root("/work/app", 1), root("/work/other", 2)],
    );
    assert_eq!(snapshot.generation, 4);
    let app = snapshot.project_for("/work/app/crates");
    assert_eq!(app.map(|p| p.threads.len()), Some(2));
    let gamma = snapshot
        .project_for("/work/other")
        .and_then(|p| p.entry("gamma"));
    assert_eq!(gamma.map(|e| e.thread_id), Some(200));
    assert_eq!(gamma.map(|e| e.text.as_ref()), Some("card gamma"));
}

#[test]
fn build_linked_sessions_are_deduplicated() {
    let snapshot = build(1, vec![row(1, "alpha")], vec![root("/work/app", 1)]);
    let entry = snapshot
        .project_for("/work/app")
        .and_then(|p| p.entry("alpha"));
    assert_eq!(entry.map(|e| e.facts.linked_sessions.len()), Some(1));
}
