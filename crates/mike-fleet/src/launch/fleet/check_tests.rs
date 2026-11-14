#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::check::{Asker, Verdict, asker, parse_tree, short, state_of};
use super::check_report::{answer, sort, text};

#[path = "check_db_tests.rs"]
pub mod mail;
use super::commit_verify::{Evidence, describe, judge};
use super::diagnostics::Diagnostic;
use super::plan::{reference, worker};
use super::plan_tests::facts;
use super::seated::Seated;
use super::tests::{changed, complete, parsed};
use super::warm::{loaded_disciplines, reference_discipline};
use crate::store::persistence::fleet::TreeCheckRow;

fn known(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(p, s)| ((*p).to_string(), (*s).to_string()))
        .collect()
}

#[test]
fn the_state_names_only_the_files_under_the_checks_paths() {
    let paths = vec!["src".to_string(), "Cargo.lock".to_string()];
    let base = known(&[("src/a.rs", "1"), ("Cargo.lock", "2"), ("docs/x.md", "3")]);
    let docs_changed = known(&[("src/a.rs", "1"), ("Cargo.lock", "2"), ("docs/x.md", "9")]);
    let code_changed = known(&[("src/a.rs", "7"), ("Cargo.lock", "2"), ("docs/x.md", "3")]);
    assert_eq!(state_of(&base, &paths), state_of(&docs_changed, &paths));
    assert_ne!(state_of(&base, &paths), state_of(&code_changed, &paths));
    assert_ne!(state_of(&base, &[]), state_of(&docs_changed, &[]));
    assert_eq!(short(&state_of(&base, &paths).unwrap()).len(), 12);
}

#[test]
fn paths_declared_as_folders_cover_their_files_and_paths_matching_nothing_name_no_state() {
    let declared = changed(json!({"check": {"command": "./check.sh",
        "paths": ["lib/", "mine/**", "`yours/`", "./"]}}));
    let covered = declared.check.covered();
    assert_eq!(covered, vec!["lib", "mine", "yours"]);
    let base = known(&[("lib/a.py", "1"), ("mine/b.py", "2"), ("docs/x.md", "3")]);
    let edited = known(&[("lib/a.py", "1"), ("mine/b.py", "8"), ("docs/x.md", "3")]);
    assert!(state_of(&base, &covered).is_some());
    assert_ne!(
        state_of(&base, &covered),
        state_of(&edited, &covered),
        "an edit under a declared folder must change the state"
    );
    assert_eq!(
        state_of(&base, &["nowhere".to_string()]),
        None,
        "a state over no file is not a state: every tree would share it"
    );
}

#[test]
fn an_asker_is_a_seated_worker_with_its_files_a_seat_with_everything_or_the_reference() {
    let mut value = complete();
    value["reference"]["session"] = json!(Uuid::from_u128(8).to_string());
    let def = parsed(&value);
    let seated = Seated::default()
        .worker("P1", Uuid::from_u128(5), "db-dev-P1")
        .with_everything(Uuid::from_u128(2), "problem-solver");
    let worker = asker(&def, &seated, Uuid::from_u128(5)).unwrap();
    assert_eq!(worker.seat, "db-dev-P1");
    assert!(!worker.everything);
    assert_eq!(
        worker.owned,
        vec![
            "crates/mike/src/store/persistence/improvement",
            "crates/mike/src/store/persistence/mod.rs"
        ]
    );
    let everything = asker(&def, &seated, Uuid::from_u128(2)).unwrap();
    assert_eq!(everything.seat, "problem-solver");
    assert!(everything.everything);
    assert_eq!(
        asker(&def, &seated, Uuid::from_u128(8)).unwrap().seat,
        "reference"
    );
    assert!(asker(&def, &seated, Uuid::from_u128(99)).is_none());
}

fn diagnostic(path: &str, line: u32) -> Diagnostic {
    Diagnostic {
        path: path.into(),
        line,
        text: format!("{path}:{line}: error"),
    }
}

#[test]
fn diagnostics_are_sorted_into_mine_other_seats_and_nobodys() {
    let def = changed(json!({}));
    let who = Asker {
        seat: "db-dev-P1".into(),
        owned: vec!["crates/mike/src/store/persistence/improvement".into()],
        everything: false,
    };
    let found = vec![
        diagnostic("crates/mike/src/store/persistence/improvement/x.rs", 3),
        diagnostic("frontend/src/routes/improvement/page.svelte", 9),
        diagnostic("scripts/run.sh", 1),
    ];
    let sorted = sort(&def, &who, &found);
    assert_eq!(sorted.mine.len(), 1);
    assert_eq!(sorted.others.len(), 1);
    assert_eq!(sorted.others[0].seat, "page");
    assert_eq!(sorted.others[0].task, "P2");
    assert_eq!(sorted.unowned.len(), 1);
    let everything = Asker {
        everything: true,
        ..who
    };
    assert_eq!(sort(&def, &everything, &found).mine.len(), 3);
}

fn row(exit: Option<i32>, output: &str) -> TreeCheckRow {
    TreeCheckRow {
        id: 1,
        run_id: "r".into(),
        state: "abcdef0123456789".into(),
        state_after: None,
        command: "make check".into(),
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        exit_code: exit,
        output: output.into(),
        askers: vec![],
        told: vec![],
    }
}

fn verdict(row: TreeCheckRow) -> Verdict {
    Verdict {
        row,
        commit: "0123456789abcdef0123456789abcdef01234567".into(),
        how: "finished".into(),
    }
}

#[test]
fn a_passed_check_at_the_commits_content_proves_it_whatever_else_is_uncommitted() {
    let def = changed(json!({"check": {"command": "make check", "paths": ["crates"]}}));
    let mine = vec!["crates/mike/src/a.rs".to_string()];
    assert_eq!(
        judge(&def, &row(Some(0), ""), &mine),
        Evidence::Proven("abcdef012345".into())
    );
    assert!(describe(&Evidence::Proven("x".into())).starts_with("proven:"));
}

#[test]
fn a_failed_check_in_the_committed_files_warns_and_elsewhere_does_not() {
    let def = changed(json!({"check": {"command": "make check", "paths": ["crates"]}}));
    let mine = vec!["crates/mike/src/a.rs".to_string()];
    let in_mine = judge(
        &def,
        &row(Some(1), "error\n  --> crates/mike/src/a.rs:4:1\n"),
        &mine,
    );
    assert_eq!(
        in_mine,
        Evidence::FailsInMine(vec![
            "- crates/mike/src/a.rs:4 --> crates/mike/src/a.rs:4:1".into()
        ])
    );
    let elsewhere = judge(
        &def,
        &row(Some(1), "error\n  --> crates/mike/src/b.rs:4:1\n"),
        &mine,
    );
    assert_eq!(elsewhere, Evidence::FailsElsewhere("abcdef012345".into()));
    assert!(describe(&elsewhere).starts_with("not proven:"));
    assert!(describe(&Evidence::Unchecked("x".into())).starts_with("not yet proven;"));
}

#[test]
fn a_committed_tree_listing_names_each_file_and_its_blob() {
    let raw = b"100644 blob 1111111111111111111111111111111111111111\tsrc/a.rs\x00100755 blob 2222222222222222222222222222222222222222\tscripts/my file.sh\0";
    let files = parse_tree(raw);
    assert_eq!(files.len(), 2);
    assert_eq!(
        files.get("scripts/my file.sh").map(String::as_str),
        Some("2222222222222222222222222222222222222222")
    );
    assert_eq!(
        state_of(&files, &["src".to_string()]),
        state_of(
            &known(&[("src/a.rs", "1111111111111111111111111111111111111111")]),
            &["src".to_string()]
        ),
        "a committed tree and a working tree with the same content name the same state"
    );
}

#[test]
fn the_reference_is_forked_from_the_first_loaded_template_and_named_to_workers() {
    let def = parsed(&complete());
    let reports = vec![
        json!({"seat": "template-db-dev", "loaded": false}),
        json!({"seat": "template-frontend-developer", "loaded": true}),
    ];
    let loaded = loaded_disciplines(&reports);
    assert_eq!(loaded, vec!["frontend-developer"]);
    assert_eq!(
        reference_discipline(&def, &loaded).as_deref(),
        Some("frontend-developer")
    );
    let wanted = changed(json!({"reference": {"discipline": "db-dev"}}));
    assert_eq!(reference_discipline(&wanted, &loaded), None);
    let off = changed(json!({"reference": {"enabled": false}}));
    assert_eq!(reference_discipline(&off, &loaded), None);

    let seat = reference(&def, &facts(), "frontend-developer", Uuid::from_u128(8)).unwrap();
    assert_eq!(seat.seat, "reference");
    assert_eq!(seat.role, "reference");
    assert_eq!(seat.discipline.as_deref(), Some("frontend-developer"));
    assert!(
        seat.prompt
            .starts_with("Your core purpose: answer the other seats' questions")
    );
    assert!(seat.prompt.contains("fleet_where"));
    assert!(reference(&off, &facts(), "frontend-developer", Uuid::from_u128(8)).is_none());

    let with_reference = changed(json!({
        "reference": {"session": Uuid::from_u128(8).to_string()},
        "check": {"command": "make check", "paths": ["crates"]}
    }));
    let brief = worker(&with_reference, &facts(), "P1", Uuid::from_u128(9))
        .unwrap()
        .prompt;
    assert!(brief.contains(&format!("reference seat, session {}", Uuid::from_u128(8))));
    assert!(brief.contains("runs the declared check") && brief.contains("`make check`"));
    let plain = worker(&def, &facts(), "P1", Uuid::from_u128(9))
        .unwrap()
        .prompt;
    assert!(!plain.contains("reference seat") && plain.contains("runs a project build or test"));
}

#[test]
fn a_check_failing_only_in_one_seats_files_gives_every_asker_a_sorted_verdict() {
    let def = parsed(&complete());
    let seated = Seated::default()
        .worker("P1", Uuid::from_u128(11), "db-dev-P1")
        .worker("P2", Uuid::from_u128(12), "frontend-developer-P2")
        .worker("P10", Uuid::from_u128(13), "frontend-developer-P10");
    let mut failed = row(
        Some(1),
        "error\n  --> frontend/src/routes/improvement/page.svelte:9:1\n",
    );
    failed.askers = vec![
        "db-dev-P1".into(),
        "frontend-developer-P2".into(),
        "frontend-developer-P10".into(),
    ];
    let seats: Vec<(u128, usize, usize)> = vec![(11, 0, 1), (12, 1, 0), (13, 0, 1)];
    for (id, mine, others) in seats {
        let who = asker(&def, &seated, Uuid::from_u128(id)).unwrap();
        let answer = answer(&def, &who, &verdict(failed.clone()));
        assert_eq!(answer.mine.len(), mine, "{}", who.seat);
        assert_eq!(answer.others.len(), others, "{}", who.seat);
        let mail = text(&answer);
        assert!(mail.contains("failed"), "{mail}");
        assert!(mail.contains("page.svelte:9"), "{mail}");
        assert_eq!(
            mail.contains("in other seats' files"),
            others == 1,
            "{mail}"
        );
    }
}

#[test]
fn a_verdict_names_the_commit_it_ran_at_and_its_state() {
    let def = parsed(&complete());
    let seated = Seated::default().worker("P1", Uuid::from_u128(11), "db-dev-P1");
    let who = asker(&def, &seated, Uuid::from_u128(11)).unwrap();
    let mut passed = row(Some(0), "");
    passed.askers = vec!["mike".into(), "db-dev-P1".into()];
    let report = answer(&def, &who, &verdict(passed));
    assert_eq!(report.commit, "0123456789abcdef0123456789abcdef01234567");
    assert_eq!(
        text(&report),
        "`make check` passed at commit 0123456789 (state abcdef012345; finished, 0 s, asked by mike, db-dev-P1)"
    );
}
