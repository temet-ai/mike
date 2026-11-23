use super::answer::{Relation, ValidMember, Validated};
use super::decide::{Action, Stmt, Verdict, plan};
use super::reach::{has_paths_frontmatter, is_always_loaded};
use chrono::{Duration, Utc};
use std::collections::HashMap;

fn base(id: i64, kind: &str) -> Stmt {
    Stmt {
        id,
        item_id: 100 + id,
        kind: kind.to_owned(),
        source_name: None,
        carrier: format!("{kind}:{id}"),
        text: format!("text {id}"),
        passage: None,
        passage_start: None,
        passage_end: None,
        stated_at: Utc::now() - Duration::days(10 - id),
        lesson_id: (kind == "lesson").then_some(id),
        lesson_active: kind == "lesson",
        current: true,
        project_id: Some(1),
        machine_wide: false,
        relative_path: None,
        always_loaded: false,
    }
}

fn file(id: i64, source: &str, path: &str, always: bool) -> Stmt {
    Stmt {
        source_name: Some(source.to_owned()),
        relative_path: Some(path.to_owned()),
        always_loaded: always,
        ..base(id, "artifact")
    }
}

fn machine(mut s: Stmt) -> Stmt {
    s.machine_wide = true;
    s.project_id = None;
    s
}

fn run(stmts: Vec<Stmt>, members: Vec<(i64, Relation)>) -> Vec<Action> {
    let v = Validated {
        members: members
            .into_iter()
            .map(|(id, relation)| ValidMember {
                id,
                relation,
                note: String::new(),
                evidence_for: None,
            })
            .collect(),
        ..Validated::default()
    };
    let map: HashMap<i64, Stmt> = stmts.into_iter().map(|s| (s.id, s)).collect();
    plan(&v, &map, &[]).actions
}

fn retired(actions: &[Action]) -> Vec<i64> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Retire { loser, .. } => Some(*loser),
            _ => None,
        })
        .collect()
}

fn related(actions: &[Action]) -> Vec<i64> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Relate { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

#[test]
fn always_loaded_means_instruction_and_rule_files_without_path_scoping() {
    assert!(is_always_loaded(Some("instructions"), Some("CLAUDE.md"), None));
    assert!(is_always_loaded(Some("instructions"), Some("sub/AGENTS.md"), None));
    assert!(is_always_loaded(Some("instructions"), Some(".claude/rules/a.md"), Some("# Rule")));
    assert!(!is_always_loaded(
        Some("instructions"),
        Some(".claude/rules/a.md"),
        Some("---\npaths:\n  - src/**\n---\nBody")
    ));
    assert!(!is_always_loaded(Some("instructions"), Some(".claude/agents/x.md"), None));
    assert!(is_always_loaded(Some("rules"), Some("rules/b.md"), Some("text")));
    assert!(!is_always_loaded(Some("skills"), Some("s/SKILL.md"), None));
    assert!(!is_always_loaded(Some("claude-memory"), Some("m.md"), None));
    assert!(has_paths_frontmatter("---\npaths: [a]\n---\n"));
    assert!(!has_paths_frontmatter("---\ntitle: x\n---\npaths: not front"));
}

#[test]
fn a_lesson_is_retired_only_when_the_coverer_reaches_every_session_it_reached() {
    let lesson = base(1, "lesson");
    let other_lesson = base(2, "lesson");
    let rule = file(3, "instructions", "CLAUDE.md", true);
    let memory = file(4, "claude-memory", "m.md", false);
    let agent = file(5, "agents", "a.md", false);
    let skill = file(6, "skills", "s.md", false);
    let mut foreign = base(7, "lesson");
    foreign.project_id = Some(2);
    let machine_rule = machine(file(8, "rules", "r.md", true));
    for ok in [2, 3, 4, 8] {
        let stmts = vec![lesson.clone(), other_lesson.clone(), rule.clone(), memory.clone(), machine_rule.clone()];
        let actions = run(stmts, vec![(1, Relation::CoveredBy(ok))]);
        assert_eq!(retired(&actions), vec![1], "coverer {ok}");
    }
    for no in [5, 6, 7] {
        let stmts = vec![lesson.clone(), agent.clone(), skill.clone(), foreign.clone()];
        let actions = run(stmts, vec![(1, Relation::CoveredBy(no))]);
        assert!(retired(&actions).is_empty(), "coverer {no}");
        assert_eq!(related(&actions), vec![1]);
    }
}

#[test]
fn a_file_statement_is_removed_only_for_the_same_file_or_a_wider_always_loaded_file() {
    let a = file(1, "skills", "s/SKILL.md", false);
    let same_doc = Stmt { item_id: a.item_id, ..file(2, "skills", "s/SKILL.md", false) };
    let project_rule = file(3, "instructions", "AGENTS.md", true);
    let machine_rule = machine(file(4, "rules", "r.md", true));
    let repo_copy = file(5, "skills", "s/SKILL.md", false);
    let home_agent = machine(file(6, "agents", "a.md", false));
    let mut other_project_rule = file(7, "instructions", "CLAUDE.md", true);
    other_project_rule.project_id = Some(2);
    let lesson = base(8, "lesson");
    for ok in [2, 3, 4] {
        let stmts = vec![a.clone(), same_doc.clone(), project_rule.clone(), machine_rule.clone()];
        assert_eq!(retired(&run(stmts, vec![(1, Relation::CoveredBy(ok))])), vec![1], "coverer {ok}");
    }
    for no in [5, 6, 7, 8] {
        let stmts = vec![a.clone(), repo_copy.clone(), home_agent.clone(), other_project_rule.clone(), lesson.clone()];
        let actions = run(stmts, vec![(1, Relation::CoveredBy(no))]);
        assert!(retired(&actions).is_empty(), "coverer {no}");
        assert_eq!(related(&actions), vec![1]);
    }
    let machine_file = machine(file(9, "agents", "x.md", false));
    let actions = run(vec![machine_file, project_rule], vec![(9, Relation::CoveredBy(3))]);
    assert!(retired(&actions).is_empty());
}

#[test]
fn a_conflict_between_a_machine_wide_and_a_project_statement_is_never_settled() {
    let mut project = file(1, "instructions", "CLAUDE.md", true);
    project.stated_at = Utc::now() - Duration::days(30);
    let global = machine(file(2, "rules", "r.md", true));
    let actions = run(
        vec![project, global],
        vec![(1, Relation::ConflictsWith(2)), (2, Relation::ConflictsWith(1))],
    );
    assert!(retired(&actions).is_empty());
    assert!(actions.iter().any(|a| matches!(a, Action::OpenConflict { .. })));

    let same_scope = run(
        vec![base(3, "lesson"), Stmt { stated_at: Utc::now() + Duration::days(5), ..base(4, "lesson") }],
        vec![(3, Relation::ConflictsWith(4)), (4, Relation::ConflictsWith(3))],
    );
    assert_eq!(retired(&same_scope), vec![3]);
}

#[test]
fn covered_by_chains_resolve_to_the_final_survivor() {
    let stmts = || vec![base(1, "lesson"), base(2, "lesson"), base(3, "lesson")];
    let actions = run(stmts(), vec![(1, Relation::CoveredBy(2)), (2, Relation::CoveredBy(3)), (3, Relation::Keep)]);
    let finals: Vec<(i64, i64)> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Retire { loser, by, verdict: Verdict::Covered, .. } => Some((*loser, *by)),
            _ => None,
        })
        .collect();
    assert_eq!(finals, vec![(1, 3), (2, 3)]);
}

#[test]
fn nothing_is_retired_for_a_coverer_that_lost_a_conflict_or_is_not_current() {
    let mut stale = base(3, "lesson");
    stale.current = false;
    let actions = run(vec![base(1, "lesson"), base(2, "lesson"), stale], vec![(1, Relation::CoveredBy(3)), (2, Relation::Keep)]);
    assert!(retired(&actions).is_empty());
    assert_eq!(related(&actions), vec![1]);

    let mut newer = base(5, "lesson");
    newer.stated_at = Utc::now();
    let mut old = base(4, "lesson");
    old.stated_at = Utc::now() - Duration::days(9);
    let covered = base(6, "lesson");
    let actions = run(
        vec![old, newer, covered],
        vec![
            (4, Relation::ConflictsWith(5)),
            (5, Relation::ConflictsWith(4)),
            (6, Relation::CoveredBy(4)),
        ],
    );
    assert_eq!(retired(&actions), vec![4]);
    assert_eq!(related(&actions), vec![6]);
}

#[test]
fn a_cover_cycle_keeps_the_oldest() {
    let actions = run(
        vec![base(1, "lesson"), base(2, "lesson"), base(3, "lesson")],
        vec![(1, Relation::CoveredBy(2)), (2, Relation::CoveredBy(3)), (3, Relation::CoveredBy(1))],
    );
    let mut gone = retired(&actions);
    gone.sort_unstable();
    assert_eq!(gone, vec![2, 3]);
}
