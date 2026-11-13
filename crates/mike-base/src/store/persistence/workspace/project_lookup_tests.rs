use super::{PathMatch, PathRow, decide, path_candidates};
use crate::store::persistence::workspace::ProjectPathResolution;

fn ranked(id: i64, path: &str, rank: i32) -> PathRow {
    PathRow {
        id,
        project_path: path.to_owned(),
        rank: Some(rank),
        last_seen: None,
    }
}

fn suffixed(id: i64, path: &str, seen_secs: i64) -> PathRow {
    PathRow {
        id,
        project_path: path.to_owned(),
        rank: None,
        last_seen: chrono::DateTime::from_timestamp(seen_secs, 0),
    }
}

fn resolved_id(resolution: &ProjectPathResolution) -> Option<i64> {
    match resolution {
        ProjectPathResolution::Resolved(id, _) => Some(*id),
        ProjectPathResolution::Ambiguous(_) | ProjectPathResolution::NotFound => None,
    }
}

#[test]
fn path_candidates_worktree_path_lists_path_then_root_then_alias() {
    let candidates = path_candidates("/home/u/repo/.claude/worktrees/w1");
    assert_eq!(
        candidates,
        vec![
            "/home/u/repo/.claude/worktrees/w1".to_owned(),
            "/home/u/repo".to_owned(),
            "repo".to_owned(),
        ]
    );
}

#[test]
fn path_candidates_relative_path_is_only_itself() {
    assert_eq!(path_candidates("u/repo"), vec!["u/repo".to_owned()]);
}

#[test]
fn decide_absolute_lowest_rank_wins() {
    let rows = vec![ranked(9, "repo", 3), ranked(4, "/home/u/repo", 2)];
    let (resolution, via_suffix) = decide("/home/u/repo/sub", PathMatch::Exact, rows);
    assert_eq!(resolved_id(&resolution), Some(4));
    assert!(!via_suffix);
}

#[test]
fn decide_relative_newest_alias_beats_exact_row() {
    let rows = vec![
        ranked(1, "repo", 1),
        suffixed(1, "repo", 0),
        suffixed(2, "/home/u/repo", 10),
        suffixed(3, "/home/v/repo", 20),
        suffixed(4, "/home/u/deep/repo", 30),
    ];
    let (resolution, _) = decide("repo", PathMatch::Exact, rows);
    assert_eq!(resolved_id(&resolution), Some(3));
}

#[test]
fn decide_exact_with_only_suffix_rows_is_not_found() {
    let rows = vec![suffixed(5, "/home/u/repo", 0)];
    let (resolution, _) = decide("u/repo", PathMatch::Exact, rows);
    assert!(matches!(resolution, ProjectPathResolution::NotFound));
}

#[test]
fn decide_or_suffix_single_row_resolves_to_its_path() {
    let rows = vec![suffixed(5, "/home/u/repo", 0)];
    let (resolution, via_suffix) = decide("u/repo", PathMatch::OrSuffix, rows);
    let ProjectPathResolution::Resolved(id, path) = resolution else {
        panic!("one suffix row must resolve, got {resolution:?}");
    };
    assert_eq!((id, path.as_str()), (5, "/home/u/repo"));
    assert!(via_suffix);
}

#[test]
fn decide_or_suffix_two_rows_is_ambiguous() {
    let rows = vec![suffixed(5, "/a/u/repo", 0), suffixed(6, "/b/u/repo", 0)];
    let (resolution, _) = decide("u/repo", PathMatch::OrSuffix, rows);
    assert!(matches!(resolution, ProjectPathResolution::Ambiguous(paths) if paths.len() == 2));
}
