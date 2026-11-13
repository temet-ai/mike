#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::checks::{Missing, check_definition};
use super::definition::{FleetDefinition, merge, natural_order};
use super::environment::{Environment, check_environment};
use super::status::{FleetHeader, NoteFacts, SeatFacts, build};
pub use super::test_fixtures::{changed, complete, parsed};

fn items(found: &[Missing]) -> Vec<&str> {
    found.iter().map(|m| m.item.as_str()).collect()
}

#[test]
fn a_complete_definition_has_nothing_missing() {
    assert_eq!(check_definition(&parsed(&complete())), vec![]);
}

#[test]
fn an_empty_definition_lists_every_required_item() {
    let found = check_definition(&FleetDefinition::default());
    let items = items(&found);
    for item in [
        "goal", "ask", "repo", "branch", "workers", "criteria", "tasks",
    ] {
        assert!(items.contains(&item), "{item} is not reported in {items:?}");
    }
}

#[test]
fn a_criterion_no_task_serves_is_reported() {
    let def = changed(json!({"criteria": {"C3": {"text": "docs exist", "check": "read them"}}}));
    assert_eq!(
        check_definition(&def),
        vec![Missing::new("criteria.C3", "no task serves this criterion")]
    );
}

#[test]
fn a_task_must_name_a_staffed_discipline_and_real_criteria() {
    let def =
        changed(json!({"tasks": {"P2": {"discipline": "rust-developer", "criteria": ["C9"]}}}));
    let found = check_definition(&def);
    assert!(found.contains(&Missing::new(
        "tasks.P2.discipline",
        "`rust-developer` has no workers"
    )));
    assert!(found.contains(&Missing::new(
        "tasks.P2.criteria",
        "`C9` is not a criterion"
    )));
}

#[test]
fn two_tasks_may_not_own_the_same_file_or_a_folder_holding_it() {
    let def = changed(
        json!({"tasks": {"P10": {"files": ["frontend/src/routes/improvement/guard/+page.svelte"]}}}),
    );
    let found = check_definition(&def);
    assert_eq!(items(&found), vec!["tasks.P10.files"]);
    assert!(found[0].problem.contains("task P2"));
}

#[test]
fn a_file_listed_as_shared_may_appear_in_several_tasks() {
    let def = changed(json!({"tasks": {"P2": {"files": [
        "frontend/src/routes/improvement/**",
        "crates/mike/src/store/persistence/mod.rs"
    ]}}}));
    assert_eq!(check_definition(&def), vec![]);
}

#[test]
fn a_task_waiting_on_itself_or_on_nothing_real_is_reported() {
    let def = changed(json!({"tasks": {"P2": {"after": ["P10"]}, "P1": {"after": ["P7"]}}}));
    let found = check_definition(&def);
    assert!(found.contains(&Missing::new("tasks.P1.after", "`P7` is not a task")));
    assert!(found.contains(&Missing::new(
        "tasks.P2.after",
        "the task ends up waiting on itself"
    )));
    assert!(found.contains(&Missing::new(
        "tasks.P10.after",
        "the task ends up waiting on itself"
    )));
}

#[test]
fn a_model_that_does_not_exist_is_reported() {
    let def = changed(json!({"retrospective": {"model": "gpt"}}));
    assert_eq!(items(&check_definition(&def)), vec!["retrospective.model"]);
}

#[test]
fn a_patch_changes_named_items_and_null_removes_one() {
    let mut value = complete();
    merge(
        &mut value,
        &json!({"goal": "new goal", "tasks": {"P10": null, "P3": {"name": "guard"}}}),
    );
    assert_eq!(value["goal"], "new goal");
    assert!(value["tasks"].get("P10").is_none());
    assert_eq!(value["tasks"]["P3"]["name"], "guard");
    assert_eq!(value["tasks"]["P1"]["name"], "schema");
}

#[test]
fn task_ids_sort_by_their_number() {
    let mut ids = vec!["P10", "P2", "P1"];
    ids.sort_by_key(|id| natural_order(id));
    assert_eq!(ids, vec!["P1", "P2", "P10"]);
}

fn environment() -> Environment {
    Environment {
        repo_exists: true,
        repo_worktree_of: None,
        agents: BTreeSet::from([
            "fleet-coordinator".to_string(),
            "fleet-observer".to_string(),
            "fleet-acceptance".to_string(),
            "db-dev".to_string(),
            "frontend-developer".to_string(),
        ]),
        files: BTreeSet::from([
            ".claude/fleet/observer-contract.md".to_string(),
            ".claude/fleet/worker-contract.md".to_string(),
        ]),
        open_surfaces: 3,
        reserved_surfaces: 0,
        max_surfaces: 16,
        backend_problem: None,
        ..Environment::default()
    }
}

#[test]
fn a_definition_that_fits_the_machine_has_nothing_missing() {
    assert_eq!(
        check_environment(&parsed(&complete()), &environment()),
        vec![]
    );
}

#[test]
fn a_missing_agent_a_missing_contract_and_too_many_seats_are_reported() {
    let mut env = environment();
    env.agents.remove("db-dev");
    env.open_surfaces = 15;
    env.backend_problem = Some("herdr: the server is not running".into());
    env.push_problem = Some("fatal: could not read Username".into());
    let found = check_environment(&parsed(&complete()), &env);
    assert_eq!(
        items(&found),
        vec!["repo", "workers.db-dev.agent", "workers", "terminal"]
    );
    assert!(found[0].problem.contains("cannot be pushed"));
    assert!(found[2].problem.contains("starts 3 seats"));
    assert!(found[2].problem.contains("room for 1"));
}

#[test]
fn seats_another_fleet_is_still_opening_count_against_the_room() {
    let mut env = environment();
    env.open_surfaces = 4;
    env.reserved_surfaces = 0;
    assert_eq!(check_environment(&parsed(&complete()), &env), vec![]);
    env.reserved_surfaces = 11;
    let found = check_environment(&parsed(&complete()), &env);
    assert_eq!(items(&found), vec!["workers"]);
    assert!(
        found[0].problem.contains("room for 1"),
        "{}",
        found[0].problem
    );
    assert!(
        found[0]
            .problem
            .contains("4 open, 11 more being opened by a fleet that is starting, limit 16")
    );
}

fn at(minute: u32) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc
        .with_ymd_and_hms(2026, 9, 29, 1, minute, 0)
        .unwrap()
}

fn seat_facts(seat: &str, role: &str, last_minute: u32, closed: bool) -> SeatFacts {
    SeatFacts {
        seat: Some(seat.into()),
        session: Some(uuid::Uuid::from_u128(
            seat.bytes().fold(7u128, |sum, byte| {
                sum.wrapping_mul(131).wrapping_add(u128::from(byte))
            }),
        )),
        role: Some(role.into()),
        agent: Some(role.into()),
        model: Some("sonnet".into()),
        opened_at: Some(at(0)),
        closed_at: closed.then(|| at(20)),
        requests: 12,
        last_request_at: Some(at(last_minute)),
        context_tokens: Some(640_000),
        live_state: None,
        last_status: None,
        first_request_cache: None,
    }
}

fn decision(minute: u32, title: &str, data: Value) -> NoteFacts {
    NoteFacts {
        note_uuid: uuid::Uuid::from_u128(u128::from(minute)),
        session_uuid: uuid::Uuid::from_u128(1000),
        role: Some("acceptance".into()),
        body: String::new(),
        tags: vec![],
        updated_at: at(minute),
        kind: "decision".into(),
        status: None,
        title: title.into(),
        data,
        created_at: at(minute),
        seat: Some("acceptance".into()),
    }
}

fn header() -> FleetHeader {
    FleetHeader {
        fleet: "0b6c1f0e-1111-4222-8333-444455556666".into(),
        slug: "judge-at-work".into(),
        state: "running".into(),
        run_id: Some("fleet-judge-at-work-0b6c1f0e".into()),
        started_at: Some(at(0)),
        finished_at: None,
        now: at(30),
        missing: vec![],
    }
}

#[test]
fn the_status_reads_task_state_from_the_newest_verdict_and_the_seats() {
    let def = changed(
        json!({"tasks": {"P3": {"name": "guard", "discipline": "db-dev",
        "criteria": ["C1"], "files": ["x"], "brief": "b"}}}),
    );
    let seats = [
        seat_facts("db-dev-P1", "db-dev", 29, false),
        seat_facts("frontend-developer-P2", "frontend-developer", 10, false),
    ];
    let notes = [
        decision(
            5,
            "FAIL C1 P1 slice 1: main does not compile",
            json!({"verdict": "fail", "package": "P1"}),
        ),
        decision(
            9,
            "PASS C1 P1 slice 1",
            json!({"verdict": "pass", "task": "P1", "slice": 1, "scope": "final", "criteria": ["C1"]}),
        ),
        decision(
            12,
            "FAIL C2 P2 slice 1",
            json!({"verdict": "fail", "tasks": ["P2"]}),
        ),
        decision(13, "Gate FAIL on c24fdfff", json!({"verdict": "fail"})),
        NoteFacts {
            note_uuid: uuid::Uuid::from_u128(100),
            session_uuid: uuid::Uuid::from_u128(1001),
            role: Some("frontend-developer".into()),
            body: String::new(),
            tags: vec![],
            updated_at: at(14),
            kind: "task".into(),
            status: Some("blocked".into()),
            title: "waiting on the schema".into(),
            data: json!({}),
            created_at: at(14),
            seat: Some("frontend-developer-P2".into()),
        },
    ];
    let status = build(header(), &def, &seats, &notes);
    let states: Vec<(&str, &str)> = status
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), t.state.as_str()))
        .collect();
    assert_eq!(
        states,
        vec![
            ("P1", "passed"),
            ("P2", "rework"),
            ("P3", "queued"),
            ("P10", "queued")
        ]
    );
    assert_eq!((status.tasks[0].passes, status.tasks[0].fails), (1, 1));
    assert_eq!(status.tasks[0].seat.as_deref(), Some("db-dev-P1"));
    assert_eq!(
        status.tasks[0].last.as_ref().unwrap().title,
        "C1 P1 slice 1"
    );
    let criteria: Vec<(&str, usize, usize)> = status
        .criteria
        .iter()
        .map(|c| (c.id.as_str(), c.passed, c.tasks))
        .collect();
    assert_eq!(criteria, vec![("C1", 1, 2), ("C2", 0, 2)]);
    assert_eq!(status.verdicts.len(), 4);
    assert_eq!(status.verdicts[0].title, "Gate FAIL on c24fdfff");
    assert_eq!(status.blocked.len(), 1);
    assert_eq!(status.blocked[0].seat, "frontend-developer-P2");
}

#[test]
fn a_seat_is_working_idle_silent_or_closed_by_its_last_request() {
    let def = parsed(&complete());
    let mut live = seat_facts("observer", "observer", 0, false);
    live.live_state = Some("blocked".into());
    let seats = [
        seat_facts("a", "db-dev", 30, false),
        seat_facts("b", "db-dev", 20, false),
        seat_facts("c", "db-dev", 0, false),
        seat_facts("d", "db-dev", 29, true),
        live,
    ];
    let mut out = seat_facts("e", "db-dev", 29, false);
    out.last_status = Some(429);
    let mut seats = seats.to_vec();
    seats.push(out);
    let mut head = header();
    head.now = at(31);
    let status = build(head, &def, &seats, &[]);
    let states: Vec<&str> = status.seats.iter().map(|s| s.state.as_str()).collect();
    assert_eq!(
        states,
        vec!["working", "idle", "silent", "closed", "blocked", "quota"]
    );
}

#[test]
fn a_task_the_coordinator_marked_done_does_not_override_failed_acceptance() {
    let def = changed(json!({"tasks": {"P1": {"state": "done"}}}));
    let notes = [decision(
        5,
        "FAIL C1 P1",
        json!({"verdict": "fail", "task": "P1"}),
    )];
    let status = build(header(), &def, &[], &notes);
    assert_eq!(status.tasks[0].state, "rework");
    assert!(!status.tasks[0].ready);
    assert_eq!(status.criteria[0].passed, 0);
}

#[test]
fn a_task_passes_only_when_every_criterion_it_serves_has_passed() {
    let def = changed(
        json!({"tasks": {"P3": {"name": "guard", "discipline": "db-dev",
        "criteria": ["C1", "C2"], "files": ["x"], "brief": "b"}}}),
    );
    let seats = [seat_facts("db-dev-P3", "db-dev", 29, false)];
    let notes = [decision(
        5,
        "PASS C1 P3 diagnosis",
        json!({"verdict": "pass", "task": "P3", "criteria": ["C1"]}),
    )];
    let status = build(header(), &def, &seats, &notes);
    let p3 = status.tasks.iter().find(|t| t.id == "P3").unwrap();
    assert_eq!(p3.state, "working");
    assert_eq!(p3.passed_criteria, vec!["C1".to_string()]);
    let credited: Vec<(&str, usize)> = status
        .criteria
        .iter()
        .map(|c| (c.id.as_str(), c.passed))
        .collect();
    assert_eq!(credited, vec![("C1", 1), ("C2", 0)]);
}

#[test]
fn a_fleet_of_one_task_is_refused() {
    let def = changed(json!({"tasks": {"P2": null, "P10": null}, "criteria": {"C2": null}}));
    let found = check_definition(&def);
    assert_eq!(items(&found), vec!["tasks"]);
    assert!(found[0].problem.contains("one session, not a fleet"));
}

#[test]
fn a_fleet_waiting_on_the_operator_says_so_and_one_at_a_standstill_says_for_how_long() {
    let def = parsed(&complete());
    let seats = [
        seat_facts("coordinator", "coordinator", 22, false),
        seat_facts("db-dev-P1", "db-dev", 10, false),
        seat_facts("template-db-dev", "template-db-dev", 29, true),
    ];
    let mut asked = decision(21, "a design question", json!({}));
    asked.status = Some("operator".into());
    let mut answered = decision(20, "OPERATOR: an old question", json!({}));
    answered.status = Some("answered".into());
    let notes = [
        answered,
        asked,
        decision(22, "Operator scope cut: this machine only", json!({})),
        decision(23, "PASS C1 P1", json!({"verdict": "pass", "task": "P1"})),
    ];
    let status = build(header(), &def, &seats, &notes);
    let titles: Vec<&str> = status
        .needs_operator
        .iter()
        .map(|n| n.title.as_str())
        .collect();
    assert_eq!(
        titles,
        vec!["a design question"],
        "a note that only mentions the operator is a record, not a request"
    );
    assert_eq!(status.every_seat_idle_seconds, Some(480));

    let at_work = [seat_facts("db-dev-P1", "db-dev", 29, false)];
    let moving = build(header(), &def, &at_work, &[]);
    assert_eq!(moving.every_seat_idle_seconds, None);
    assert!(moving.needs_operator.is_empty());
}
