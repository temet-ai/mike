use serde_json::json;

use super::*;
use crate::launch::fleet::tests::changed;

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap_or_default()
}

fn def() -> FleetDefinition {
    changed(json!({
        "repo": "/srv/project",
        "tasks": {
            "T1": {"files": ["crates/a/src/one.rs"]},
            "T2": {"files": ["crates/a/src/two.rs"]}
        }
    }))
}

fn roster() -> Roster {
    Roster {
        sessions: BTreeMap::from([
            ("T1".to_string(), Uuid::from_u128(11)),
            ("T2".to_string(), Uuid::from_u128(12)),
        ]),
        reference: Some(Uuid::from_u128(40)),
    }
}

fn uncommitted() -> BTreeMap<String, Option<Author>> {
    BTreeMap::from([
        (
            "crates/a/src/one.rs".to_string(),
            Some(Author {
                who: "rust-T1".into(),
                session: Some(Uuid::from_u128(11)),
            }),
        ),
        (
            "crates/a/src/two.rs".to_string(),
            Some(Author {
                who: "someone else".into(),
                session: None,
            }),
        ),
    ])
}

const OUTPUT: &str = "\
error[E0425]: cannot find value `x` in this scope
  --> crates/a/src/one.rs:12:5
error[E0308]: mismatched types
  --> crates/a/src/two.rs:7:9
error[E0432]: unresolved import
  --> crates/a/src/committed.rs:3:5
";

fn verdict(now: &str) -> Verdict<'static> {
    Verdict {
        run: "run-1",
        sha: "abc123",
        now: at(now),
    }
}

fn as_events(outcome: &Outcome, first_id: i64, when: &str) -> Vec<FleetEvent> {
    outcome
        .events
        .iter()
        .zip(first_id..)
        .map(|(new, id)| FleetEvent {
            id,
            run_id: new.run_id.clone(),
            task: new.task.clone(),
            session_uuid: new.session_uuid,
            kind: EventKind::TreeBrokenBy,
            payload: new.payload.body().unwrap_or_default(),
            occurred_at: at(when),
        })
        .collect()
}

fn broken_now() -> BTreeMap<String, Vec<Diagnostic>> {
    let found = diagnostics::parse(OUTPUT, "/srv/project");
    attribute(&def(), &roster(), &found, &uncommitted())
}

#[test]
fn a_seeded_uncommitted_break_is_attributed_to_its_seat_in_one_evaluation() {
    let broken = broken_now();
    assert_eq!(
        broken.keys().cloned().collect::<Vec<_>>(),
        vec!["T1".to_string(), "T2".to_string()],
        "the committed file is no seat's uncommitted change; the other author falls back to the file's owner"
    );
    let outcome = evaluate(
        &def(),
        &roster(),
        &Held::default(),
        &broken,
        &verdict("2026-10-22T02:00:00Z"),
    );
    assert_eq!(outcome.events.len(), 2);
    assert_eq!(outcome.events[0].task.as_deref(), Some("T1"));
    assert!(matches!(
        &outcome.events[0].payload,
        EventPayload::TreeBrokenBy(b) if b.paths == vec!["crates/a/src/one.rs".to_string()] && b.sha == "abc123"
    ));
}

#[test]
fn each_seat_is_told_once_per_break_with_its_failing_lines() {
    let outcome = evaluate(
        &def(),
        &roster(),
        &Held::default(),
        &broken_now(),
        &verdict("2026-10-22T02:00:00Z"),
    );
    assert_eq!(outcome.notices.len(), 2);
    let first = &outcome.notices[0];
    assert_eq!(first.to, Uuid::from_u128(11));
    assert!(first.body.contains("crates/a/src/one.rs:12"));
    assert!(!first.body.contains("two.rs"));
    let later = evaluate(
        &def(),
        &roster(),
        &held(
            &as_events(&outcome, 1, "2026-10-22T02:00:00Z"),
            at("2026-10-22T02:05:00Z"),
        ),
        &broken_now(),
        &verdict("2026-10-22T02:05:00Z"),
    );
    assert!(later.events.is_empty(), "an unchanged break adds no event");
    assert_eq!(
        later.notices[0].key, first.key,
        "the told key holds for the whole break"
    );
}

#[test]
fn the_status_line_counts_minutes_and_names_the_tasks() {
    let outcome = evaluate(
        &def(),
        &roster(),
        &Held::default(),
        &broken_now(),
        &verdict("2026-10-22T02:00:00Z"),
    );
    let events = as_events(&outcome, 1, "2026-10-22T02:00:00Z");
    let broken = state(&events, at("2026-10-22T02:07:00Z"));
    let Some(broken) = broken else {
        panic!("the break is held");
    };
    assert_eq!(broken.tasks, vec!["T1".to_string(), "T2".to_string()]);
    assert_eq!(
        line(&broken, at("2026-10-22T02:07:30Z")),
        "the tree has not compiled for 7 minutes; the failing lines are in files T1 and T2 have not committed"
    );
}

#[test]
fn a_compiling_tree_clears_the_break() {
    let outcome = evaluate(
        &def(),
        &roster(),
        &Held::default(),
        &broken_now(),
        &verdict("2026-10-22T02:00:00Z"),
    );
    let mut events = as_events(&outcome, 1, "2026-10-22T02:00:00Z");
    let prior = held(&events, at("2026-10-22T02:01:00Z"));
    let cleared = evaluate(
        &def(),
        &roster(),
        &prior,
        &BTreeMap::new(),
        &verdict("2026-10-22T02:09:00Z"),
    );
    assert_eq!(cleared.events.len(), 2);
    events.extend(as_events(&cleared, 10, "2026-10-22T02:09:00Z"));
    assert_eq!(state(&events, at("2026-10-22T02:10:00Z")), None);
}

#[test]
fn the_reference_seat_hears_of_a_break_held_ten_minutes_and_only_then() {
    let outcome = evaluate(
        &def(),
        &roster(),
        &Held::default(),
        &broken_now(),
        &verdict("2026-10-22T02:00:00Z"),
    );
    let events = as_events(&outcome, 1, "2026-10-22T02:00:00Z");
    let prior = held(&events, at("2026-10-22T02:09:00Z"));
    assert!(tripwires(&roster(), &prior, at("2026-10-22T02:09:00Z")).is_empty());
    let late = tripwires(&roster(), &prior, at("2026-10-22T02:10:30Z"));
    assert_eq!(late.len(), 2);
    assert_eq!(late[0].to, Uuid::from_u128(40));
    assert!(late[0].body.contains("of task T1"));
    assert!(late[0].body.contains("crates/a/src/one.rs"));
}

#[test]
fn porcelain_lines_give_the_new_path_of_a_rename_and_untracked_files() {
    let porcelain =
        " M crates/a/src/one.rs\n?? crates/a/src/new.rs\nR  old.rs -> crates/a/src/moved.rs\n";
    assert_eq!(
        dirty_from_porcelain(porcelain),
        vec![
            "crates/a/src/one.rs".to_string(),
            "crates/a/src/new.rs".to_string(),
            "crates/a/src/moved.rs".to_string()
        ]
    );
}
