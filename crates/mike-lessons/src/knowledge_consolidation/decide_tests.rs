use super::answer::{MergeGroup, Relation, ValidMember, Validated};
use super::decide::{Action, EvidenceItem, Rule, Stmt, Verdict, acceptable_merges, decide_conflict, plan};
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};

fn at(days_ago: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(days_ago)
}

fn stmt(id: i64, kind: &str, stated_days_ago: i64, text: &str) -> Stmt {
    Stmt {
        id,
        item_id: id + 1000,
        kind: kind.to_owned(),
        source_name: (kind == "artifact").then(|| "rules".to_owned()),
        carrier: format!("{kind}:{id}"),
        text: text.to_owned(),
        passage: None,
        passage_start: None,
        passage_end: None,
        stated_at: at(stated_days_ago),
        lesson_id: (kind == "lesson").then_some(id + 500),
        lesson_active: kind == "lesson",
        current: true,
        project_id: Some(1),
        machine_wide: false,
        relative_path: None,
        always_loaded: false,
    }
}

fn evidence(kind: &str, days_ago: i64) -> EvidenceItem {
    EvidenceItem {
        id: 1_000_000,
        reference: "correction:1".to_owned(),
        kind: kind.to_owned(),
        dated_at: at(days_ago),
    }
}

#[test]
fn operator_evidence_newer_than_both_statements_decides_for_the_named_statement() {
    let old = stmt(1, "lesson", 10, "Use git.");
    let newer = stmt(2, "artifact", 3, "Use jj.");
    for kind in ["correction", "unmet_ask", "question_answer"] {
        let won = decide_conflict(&old, &newer, Some(1), &[evidence(kind, 1)]);
        assert_eq!(won.map(|w| (w.winner, w.loser, w.rule)), Some((1, 2, Rule::OperatorEvidence)));
    }
}

#[test]
fn evidence_that_is_not_the_operators_or_not_newer_than_both_does_not_decide() {
    let a = stmt(1, "lesson", 10, "Use git.");
    let b = stmt(2, "lesson", 3, "Use jj.");
    let note = decide_conflict(&a, &b, Some(1), &[evidence("note", 1)]);
    assert_eq!(note.map(|w| w.rule), Some(Rule::Newer));
    let stale = decide_conflict(&a, &b, Some(1), &[evidence("correction", 5)]);
    assert_eq!(stale.map(|w| (w.winner, w.rule)), Some((2, Rule::Newer)));
    let unnamed = decide_conflict(&a, &b, None, &[evidence("correction", 1)]);
    assert_eq!(unnamed.map(|w| w.rule), Some(Rule::Newer));
}

#[test]
fn a_statement_more_than_a_day_newer_wins_and_within_a_day_it_does_not() {
    let older = stmt(1, "lesson", 5, "A");
    let newer = stmt(2, "lesson", 2, "B");
    let won = decide_conflict(&older, &newer, None, &[]);
    assert_eq!(won.map(|w| (w.winner, w.loser, w.rule)), Some((2, 1, Rule::Newer)));
    let reversed = decide_conflict(&newer, &older, None, &[]);
    assert_eq!(reversed.map(|w| w.winner), Some(2));
    let close_a = stmt(3, "lesson", 0, "A");
    let mut close_b = stmt(4, "lesson", 0, "B");
    close_b.stated_at = close_a.stated_at - Duration::hours(20);
    assert_eq!(decide_conflict(&close_a, &close_b, None, &[]), None);
}

#[test]
fn within_a_day_a_file_beats_a_lesson_and_two_of_a_kind_stay_open() {
    let lesson = stmt(1, "lesson", 0, "A");
    let file = stmt(2, "artifact", 0, "B");
    let won = decide_conflict(&lesson, &file, None, &[]);
    assert_eq!(won.map(|w| (w.winner, w.loser, w.rule)), Some((2, 1, Rule::FileOverLesson)));
    let other = stmt(3, "artifact", 0, "C");
    assert_eq!(decide_conflict(&file, &other, None, &[]), None);
    assert_eq!(decide_conflict(&lesson, &stmt(4, "lesson", 0, "D"), None, &[]), None);
}

fn validated(members: Vec<ValidMember>) -> Validated {
    Validated {
        members,
        ..Validated::default()
    }
}

fn valid(id: i64, relation: Relation) -> ValidMember {
    ValidMember {
        id,
        relation,
        note: String::new(),
        evidence_for: None,
    }
}

fn map(stmts: Vec<Stmt>) -> HashMap<i64, Stmt> {
    stmts.into_iter().map(|s| (s.id, s)).collect()
}

#[test]
fn the_plan_retires_covered_statements_and_settles_each_conflict_pair_once() {
    let stmts = map(vec![
        stmt(1, "lesson", 0, "Use jj."),
        Stmt { always_loaded: true, ..stmt(2, "artifact", 0, "Use jj always.") },
        stmt(3, "lesson", 9, "Use git."),
        stmt(4, "lesson", 1, "Use jj, never git."),
        stmt(5, "lesson", 0, "Other"),
        stmt(6, "lesson", 0, "Stray"),
    ]);
    let v = validated(vec![
        valid(1, Relation::CoveredBy(2)),
        valid(2, Relation::Keep),
        valid(3, Relation::ConflictsWith(4)),
        valid(4, Relation::ConflictsWith(3)),
        valid(5, Relation::AddsTo(2)),
        valid(6, Relation::Elsewhere),
    ]);
    let p = plan(&v, &stmts, &[]);
    let retired: Vec<(i64, i64, Verdict)> = p
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Retire { loser, by, verdict, .. } => Some((*loser, *by, *verdict)),
            _ => None,
        })
        .collect();
    let mut retired = retired;
    retired.sort_by_key(|r| r.0);
    assert_eq!(retired, vec![(1, 2, Verdict::Covered), (3, 4, Verdict::Superseded)]);
    assert!(p.actions.iter().any(|a| matches!(a, Action::Keep { id: 2 })));
    assert!(p.actions.iter().any(|a| matches!(a, Action::Relate { id: 5, counterpart: 2, .. })));
    assert!(p.actions.iter().any(|a| matches!(a, Action::Misgroup { id: 6, .. })));
}

#[test]
fn an_unsettled_conflict_stays_open_and_retires_nothing() {
    let stmts = map(vec![stmt(1, "lesson", 0, "A"), stmt(2, "lesson", 0, "B")]);
    let v = validated(vec![valid(1, Relation::ConflictsWith(2)), valid(2, Relation::ConflictsWith(1))]);
    let p = plan(&v, &stmts, &[]);
    assert_eq!(p.actions.len(), 1);
    assert!(matches!(p.actions[0], Action::OpenConflict { id: 1, counterpart: 2, .. }));
}

fn group(ids: &[i64], text: &str) -> MergeGroup {
    MergeGroup {
        ids: ids.to_vec(),
        text: text.to_owned(),
    }
}

fn merge_input(groups: Vec<MergeGroup>) -> Validated {
    Validated {
        merges: groups,
        ..Validated::default()
    }
}

#[test]
fn a_merge_needs_active_lessons_a_text_no_longer_than_theirs_and_no_overlap() {
    let mut inactive = stmt(3, "lesson", 0, "Gone lesson text.");
    inactive.lesson_active = false;
    let stmts = map(vec![
        stmt(1, "lesson", 0, "Run clippy."),
        stmt(2, "lesson", 0, "Run clippy before commit."),
        inactive,
        stmt(4, "artifact", 0, "A file statement."),
        stmt(5, "lesson", 0, "Another lesson."),
    ]);
    let retired = HashSet::new();
    let ok = acceptable_merges(&merge_input(vec![group(&[1, 2], "Run clippy before commit.")]), &stmts, &retired);
    assert_eq!(ok.len(), 1);
    for bad in [
        group(&[1, 2], &"x".repeat(60)),
        group(&[1], "Run clippy."),
        group(&[1, 3], "Run."),
        group(&[1, 4], "Run."),
        group(&[1, 404], "Run."),
        group(&[1, 2], "   "),
    ] {
        assert!(acceptable_merges(&merge_input(vec![bad.clone()]), &stmts, &retired).is_empty(), "{bad:?}");
    }
    let overlapping = acceptable_merges(
        &merge_input(vec![group(&[1, 2], "Run clippy first."), group(&[2, 5], "Run.")]),
        &stmts,
        &retired,
    );
    assert_eq!(overlapping.len(), 1);
    let one_retired: HashSet<i64> = [2].into_iter().collect();
    assert!(acceptable_merges(&merge_input(vec![group(&[1, 2], "Run.")]), &stmts, &one_retired).is_empty());
}
