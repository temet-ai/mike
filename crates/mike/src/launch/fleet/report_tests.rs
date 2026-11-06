use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::{FleetDefinition, merge};
use super::report::{
    CHECK_FAILED, CHECK_NOT_YET, CHECK_PASSED, CommitFacts, Exercised, ReportRequest,
    VerdictRequest, acceptance_mail, check_at_commit, check_report, check_verdict, decision_data,
    newest_commit_of, report_body, waiting,
};
use super::seated::Seated;
use super::status::{FleetHeader, FleetStatus, NoteFacts, build};
use super::tests::{complete, parsed};
use crate::change_record::{ChangeEvent, ChangeKind};

const FLEET: &str = "00000000-0000-0000-0000-0000000000f1";
const ACCEPTANCE: u128 = 1;
const WORKER: u128 = 14;
const OTHER: u128 = 15;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_default()
}

fn seated() -> Value {
    let mut value = complete();
    merge(
        &mut value,
        &json!({
            "coordinator": {"session": id(2).to_string()},
            "observer": {"session": id(3).to_string()},
            "acceptance": {"session": id(ACCEPTANCE).to_string()},
            "tasks": {
                "P1": {"seat": "db-dev-P1", "session": id(WORKER).to_string()},
                "P2": {"seat": "frontend-developer-P2", "session": id(OTHER).to_string()}
            }
        }),
    );
    value
}

fn def() -> FleetDefinition {
    parsed(&seated())
}

fn held() -> Seated {
    Seated::default()
        .worker("P1", id(WORKER), "db-dev-P1")
        .worker("P2", id(OTHER), "frontend-developer-P2")
}

fn report_request(session: u128, task: &str) -> ReportRequest {
    ReportRequest {
        session: Some(id(session)),
        task: task.into(),
        slice: "the table and its reads".into(),
        criteria: vec!["C1".into()],
        summary: String::new(),
        look_at: "I am confident this is complete and excellent.".into(),
        evidence: vec!["crates/mike/src/store/persistence/improvement/mod.rs".into()],
        sha: Some("0123456789abcdef".into()),
        interface: false,
        exercised: Some(Exercised {
            command: "scripts/cx nextest run -p mike --lib floor".into(),
            exit: Some(0),
            output_ref: "artifacts/run.txt".into(),
        }),
        tests_changed: Some(vec!["none".into()]),
        unestablished: Some(Vec::new()),
    }
}

fn verdict_request(session: u128, verdict: &str, scope: &str) -> VerdictRequest {
    VerdictRequest {
        session: Some(id(session)),
        task: "P1".into(),
        criteria: vec!["C1".into()],
        verdict: verdict.into(),
        scope: scope.into(),
        reason: "the table is read back whole by the route".into(),
        report_note: None,
        regressions: vec![],
    }
}

fn note(n: u128, author: u128, kind: &str, data: Value, time: i64) -> NoteFacts {
    NoteFacts {
        note_uuid: id(n),
        session_uuid: id(author),
        role: None,
        body: String::new(),
        tags: vec![format!("fleet:{FLEET}")],
        updated_at: at(time),
        kind: kind.into(),
        status: None,
        title: String::new(),
        data,
        created_at: at(time),
        seat: None,
    }
}

fn status(def: &FleetDefinition, notes: &[NoteFacts]) -> FleetStatus {
    build(
        FleetHeader {
            fleet: FLEET.into(),
            slug: "report".into(),
            state: "running".into(),
            run_id: Some("run".into()),
            started_at: Some(at(0)),
            finished_at: None,
            now: at(100),
            missing: vec![],
        },
        def,
        &[],
        notes,
    )
}

fn state_of(found: &FleetStatus, task: &str) -> String {
    found
        .tasks
        .iter()
        .find(|t| t.id == task)
        .map(|t| t.state.clone())
        .unwrap_or_default()
}

#[test]
fn a_report_is_refused_for_a_task_the_session_does_not_hold_and_for_a_criterion_it_does_not_serve()
{
    let def = def();
    let Err(why) = check_report(&def, &held(), &report_request(OTHER, "P1")) else {
        panic!("a seat cannot report another seat's task");
    };
    assert!(why.contains("not held by session"), "{why}");
    let mut wrong = report_request(WORKER, "P1");
    wrong.criteria = vec!["C2".into()];
    let Err(why) = check_report(&def, &held(), &wrong) else {
        panic!("C2 is not P1's criterion");
    };
    assert!(why.contains("`C2` is not a criterion of task P1"), "{why}");
    let mut nameless = report_request(WORKER, "P1");
    nameless.slice = " ".into();
    assert!(check_report(&def, &held(), &nameless).is_err());
    assert!(check_report(&def, &held(), &report_request(WORKER, "P9")).is_err());
    let Ok((_, criteria)) = check_report(&def, &held(), &report_request(WORKER, "P1")) else {
        panic!("the holder may report its own task");
    };
    assert_eq!(criteria, vec!["C1".to_string()]);
}

#[test]
fn only_the_acceptance_session_records_a_verdict_and_every_word_is_checked() {
    let def = def();
    let Err(why) = check_verdict(&def, &verdict_request(WORKER, "pass", "final")) else {
        panic!("a worker cannot judge");
    };
    assert!(why.contains("not this fleet's acceptance seat"), "{why}");
    for (verdict, scope) in [("maybe", "final"), ("pass", "whole")] {
        assert!(check_verdict(&def, &verdict_request(ACCEPTANCE, verdict, scope)).is_err());
    }
    let mut silent = verdict_request(ACCEPTANCE, "fail", "slice");
    silent.reason = String::new();
    assert!(check_verdict(&def, &silent).is_err());
    let Ok((_, judged)) = check_verdict(&def, &verdict_request(ACCEPTANCE, "PASS", "Final")) else {
        panic!("acceptance may judge");
    };
    assert_eq!((judged.verdict, judged.scope), ("pass", "final"));
}

#[test]
fn a_verdict_written_by_fleet_verdict_is_counted_by_the_register_and_moves_the_task() {
    let def = def();
    let Ok((_, judged)) = check_verdict(&def, &verdict_request(ACCEPTANCE, "pass", "final")) else {
        panic!("acceptance may judge");
    };
    let data = decision_data("P1", &judged, ("it holds", Some(id(90)), Some(3)));
    assert_eq!(data["brief_version"], 3);
    assert_eq!(data["report_note"], id(90).to_string());
    let reported = note(90, WORKER, "report", json!({"task": "P1"}), 10);
    let before = status(&def, std::slice::from_ref(&reported));
    assert_eq!(state_of(&before, "P1"), "awaiting acceptance");
    let decided = note(91, ACCEPTANCE, "decision", data, 20);
    let after = status(&def, &[reported, decided]);
    let p1 = after.tasks.iter().find(|t| t.id == "P1");
    assert_eq!(
        p1.map(|t| t.passed_criteria.clone()),
        Some(vec!["C1".to_string()])
    );
    assert_eq!(state_of(&after, "P1"), "passed");
    assert_eq!(p1.map(|t| t.passes), Some(1));
}

#[test]
fn a_slice_pass_does_not_pass_the_criterion_and_a_fail_sends_the_task_to_rework() {
    let def = def();
    let slice = |verdict: &str| {
        let Ok((_, judged)) = check_verdict(&def, &verdict_request(ACCEPTANCE, verdict, "slice"))
        else {
            panic!("acceptance may judge");
        };
        decision_data("P1", &judged, ("why", None, None))
    };
    let passed = status(&def, &[note(91, ACCEPTANCE, "decision", slice("pass"), 20)]);
    assert!(passed.tasks[0].passed_criteria.is_empty());
    let failed = status(&def, &[note(92, ACCEPTANCE, "decision", slice("fail"), 20)]);
    assert_eq!(state_of(&failed, "P1"), "rework");
}

#[test]
fn a_task_state_written_into_the_definition_is_ignored() {
    for (hand_set, shown) in [
        ("done", "queued"),
        ("passed", "queued"),
        ("blocked on T2", "queued"),
        ("not started: no room", "queued"),
        ("starting", "queued"),
    ] {
        let mut value = seated();
        merge(&mut value, &json!({"tasks": {"P1": {"state": hand_set}}}));
        assert_eq!(
            state_of(&status(&parsed(&value), &[]), "P1"),
            shown,
            "{hand_set}"
        );
    }
}

fn incomplete_message(mutate: impl FnOnce(&mut ReportRequest)) -> String {
    let mut req = report_request(WORKER, "P1");
    mutate(&mut req);
    match check_report(&def(), &held(), &req) {
        Err(why) => why,
        Ok(_) => String::new(),
    }
}

type Mutation = Box<dyn FnOnce(&mut ReportRequest)>;

#[test]
fn a_report_missing_a_typed_field_is_refused_naming_it() {
    let cases: [(&str, Mutation); 8] = [
        ("`commit`", Box::new(|r| r.sha = None)),
        ("`criteria`", Box::new(|r| r.criteria.clear())),
        ("`exercised`", Box::new(|r| r.exercised = None)),
        (
            "`exercised.command`",
            Box::new(|r| r.exercised = Some(Exercised::default())),
        ),
        (
            "`exercised.exit`",
            Box::new(|r| {
                r.exercised = Some(Exercised {
                    command: "c".into(),
                    exit: None,
                    output_ref: "o".into(),
                });
            }),
        ),
        (
            "`exercised.output_ref`",
            Box::new(|r| {
                r.exercised = Some(Exercised {
                    command: "c".into(),
                    exit: Some(0),
                    output_ref: " ".into(),
                });
            }),
        ),
        ("`tests_changed`", Box::new(|r| r.tests_changed = None)),
        ("`unestablished`", Box::new(|r| r.unestablished = None)),
    ];
    for (field, mutate) in cases {
        let why = incomplete_message(mutate);
        assert!(
            why.starts_with(&format!("fleet_report is incomplete: {field}")),
            "{field}: {why}"
        );
    }
}

#[test]
fn tests_changed_takes_paths_or_none_but_not_both() -> anyhow::Result<()> {
    let none: ReportRequest = serde_json::from_value(json!({"tests_changed": "none"}))?;
    assert_eq!(none.tests_changed, Some(vec!["none".to_string()]));
    let both =
        incomplete_message(|r| r.tests_changed = Some(vec!["none".into(), "a_tests.rs".into()]));
    assert!(both.contains("`tests_changed` lists"), "{both}");
    let empty = incomplete_message(|r| r.tests_changed = Some(Vec::new()));
    assert!(empty.contains("`tests_changed` is missing"), "{empty}");
    let paths = incomplete_message(|r| r.tests_changed = Some(vec!["a_tests.rs".into()]));
    assert!(paths.is_empty(), "{paths}");
    let commit: ReportRequest = serde_json::from_value(json!({"commit": "abc"}))?;
    assert_eq!(commit.sha.as_deref(), Some("abc"));
    Ok(())
}

#[test]
fn a_report_waits_until_a_later_acceptance_decision_names_its_task() {
    let def = def();
    let notes = vec![
        note(90, WORKER, "report", json!({"task": "P1"}), 10),
        note(91, OTHER, "report", json!({"task": "P2"}), 12),
        note(
            92,
            ACCEPTANCE,
            "decision",
            json!({"readiness": "ready", "task": "P2"}),
            15,
        ),
        note(93, WORKER, "report", json!({"task": "P1"}), 30),
    ];
    let found = waiting(&def, &notes);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].task.as_str(), found[0].reports), ("P1", 2));
    assert_eq!((found[0].since, found[0].newest), (at(10), id(93)));
    assert_eq!(found[0].seat, "db-dev-P1");
}

fn check(sha: &str, passed: bool, in_detail: bool) -> ChangeEvent {
    ChangeEvent {
        id: 1,
        project_id: 1,
        checkout: "/srv/project".into(),
        at: Utc::now(),
        kind: ChangeKind::Check,
        session_id: None,
        run_id: None,
        task: None,
        paths: vec![],
        subject: String::new(),
        detail: if in_detail {
            json!({"commit": sha, "passed": passed})
        } else {
            json!({"passed": passed})
        },
        before_sha: None,
        after_sha: (!in_detail).then(|| sha.to_string()),
    }
}

#[test]
fn the_check_at_a_commit_is_read_from_the_record_by_its_sha() {
    let events = vec![check("aaa", false, true), check("bbb", true, false)];
    assert_eq!(check_at_commit(&events, "aaa"), CHECK_FAILED);
    assert_eq!(check_at_commit(&events, "bbb"), CHECK_PASSED);
    assert_eq!(check_at_commit(&events, "ccc"), CHECK_NOT_YET);
}

#[test]
fn the_newest_commit_is_the_seats_latest_commit_note_for_the_task() {
    let notes = vec![
        (json!({"task": "P1", "sha": "old"}), at(10)),
        (json!({"task": "P2", "sha": "theirs"}), at(40)),
        (json!({"task": "P1", "sha": "new"}), at(30)),
    ];
    assert_eq!(newest_commit_of(&notes, "P1").as_deref(), Some("new"));
    assert_eq!(newest_commit_of(&notes, "P9"), None);
}

#[test]
fn acceptance_is_shown_the_commit_the_check_and_the_note_never_the_seats_prose() {
    let facts = CommitFacts {
        commit: Some("0123456789abcdef".into()),
        check: CHECK_PASSED.into(),
        checked_after_last_change: None,
        brief_version: Some(2),
        check_command: "compile check (no tests): cargo check".into(),
        proof: Default::default(),
    };
    let req = report_request(WORKER, "P1");
    let text = acceptance_mail("db-dev-P1", &req, &["C1".into()], &facts, id(90));
    for wanted in [
        "Task P1 (db-dev-P1), slice \"the table and its reads\", criteria C1.",
        "Report note: 00000000-0000-0000-0000-00000000005a.",
        "Commit: 0123456789abcdef.",
        "Check at that commit: passed.",
        "The check ran after the seat's last change: unknown.",
        "Brief version the seat read: 2.",
        "Evidence: crates/mike/src/store/persistence/improvement/mod.rs.",
        "fleet_verdict",
    ] {
        assert!(text.contains(wanted), "{wanted} missing from {text}");
    }
    assert!(!text.contains("confident"), "{text}");
}

#[test]
fn a_report_summary_is_written_into_the_note_body_with_what_to_look_at() {
    let mut req = report_request(WORKER, "P1");
    req.summary = "  ran `make test-p p=mike`: 12 passed; no test files modified  ".into();
    assert_eq!(
        report_body(&req),
        "ran `make test-p p=mike`: 12 passed; no test files modified\n\nWhat to look at: I am confident this is complete and excellent."
    );
    req.look_at = " ".into();
    assert_eq!(
        report_body(&req),
        "ran `make test-p p=mike`: 12 passed; no test files modified"
    );
    req.summary.clear();
    req.look_at = "the route test".into();
    assert_eq!(report_body(&req), "the route test");
}

#[path = "report_tests_more.rs"]
mod more;
