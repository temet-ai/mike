#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::collections::BTreeSet;

use serde_json::json;
use uuid::Uuid;

use super::environment::starts;
use super::plan::{
    ControlSeats, ready_tasks, template, template_name, waiting_tasks, warm_disciplines, worker,
};
use super::staff::{no_room_state, passed_tasks};

#[test]
fn a_full_surface_cap_is_named_in_the_tasks_state_and_does_not_unseat_them() {
    let state = no_room_state(17, 16);
    assert!(
        state.starts_with("waiting: no free terminal surface (17 of 16 open)"),
        "{state}"
    );
    assert!(state.contains("launch_max_surfaces"), "{state}");
}
use super::start::control_patch;
use super::status::{FleetHeader, build};
pub use super::test_fixtures::{acceptance, coordinator, facts, seats};
use super::tests::{changed, complete, parsed};

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

#[test]
fn every_task_that_waits_on_nothing_is_ready_whatever_its_discipline() {
    let def = changed(
        json!({"tasks": {"P3": {"name": "stories", "discipline": "frontend-developer",
        "criteria": ["C2"], "files": ["frontend/src/stories/"], "brief": "Write them."}}}),
    );
    assert_eq!(ready_tasks(&def, &set(&[])), vec!["P1", "P2", "P3"]);
    assert_eq!(waiting_tasks(&def), vec!["P10"]);
}

#[test]
fn a_task_becomes_ready_when_every_dependency_is_ready() {
    let def = changed(json!({"tasks": {"P10": {"after": ["P1", "P2"]}}}));
    assert_eq!(ready_tasks(&def, &set(&["P2"])), vec!["P1", "P2"]);
    assert_eq!(
        ready_tasks(&def, &set(&["P1", "P2"])),
        vec!["P1", "P2", "P10"]
    );
}

#[test]
fn ready_reads_only_after_a_seat_or_state_left_in_the_definition_changes_nothing() {
    let plain = changed(json!({}));
    let stale = changed(json!({"tasks": {
        "P1": {"seat": "db-dev-P1", "session": "s1", "state": "working"},
        "P2": {"state": "done"}}}));
    assert_eq!(
        ready_tasks(&stale, &set(&[])),
        ready_tasks(&plain, &set(&[]))
    );
    assert_eq!(ready_tasks(&stale, &set(&[])), vec!["P1", "P2"]);
}

#[test]
fn every_discipline_with_a_task_gets_a_warm_template() {
    let def = parsed(&complete());
    assert_eq!(warm_disciplines(&def), vec!["db-dev", "frontend-developer"]);
    assert_eq!(
        template_name("fleet-run-1", "frontend-developer"),
        "fleet-run-1:frontend-developer"
    );
}

#[test]
fn a_fleet_plan_opens_no_control_seat_and_a_brief_names_none() {
    let def = parsed(&complete());
    let worker_prompt = worker(&def, &facts(), "P2", Uuid::from_u128(9))
        .unwrap()
        .prompt;
    for gone in [
        "Coordinator session",
        "Acceptance session",
        "Observer session",
    ] {
        assert!(!worker_prompt.contains(gone), "{gone}");
    }
    assert_eq!(starts(&def).control_seats, 0);
}

#[test]
fn a_worker_is_told_its_task_and_who_to_talk_to() {
    let def = parsed(&complete());
    let session = Uuid::from_u128(9);
    let seat = worker(&def, &facts(), "P2", session).unwrap();
    assert_eq!(seat.seat, "frontend-developer-P2");
    assert_eq!(seat.role, "frontend-developer");
    assert_eq!(seat.task.as_deref(), Some("P2"));
    assert_eq!(seat.session, session);
    assert!(
        seat.prompt
            .starts_with("Your core purpose: finish your one task")
    );
    assert!(seat.prompt.contains("Your task is P2: page."));
    assert!(seat.prompt.contains("Build the page."));
    assert!(seat.prompt.contains("- C2: the page lists proposals"));
    assert!(!seat.prompt.contains("- C1:"));
    assert!(seat.prompt.contains("- frontend/src/routes/improvement/**"));
    assert!(seat.prompt.contains("mike's problem-solver decides it"));
    assert!(seat.prompt.contains("runs the task's floor at that commit"));
    assert!(worker(&def, &facts(), "P99", session).is_none());
}

#[test]
fn a_worker_is_told_every_other_tasks_seat_and_session() {
    let other = Uuid::from_u128(77);
    let def =
        changed(json!({"tasks": {"P1": {"seat": "db-dev-P1", "session": other.to_string()}}}));
    let seat = worker(&def, &facts(), "P2", Uuid::from_u128(9)).unwrap();
    assert!(
        seat.prompt
            .contains(&format!("- P1 schema: db-dev-P1, session {other}")),
        "{}",
        seat.prompt
    );
    assert!(
        !seat.prompt.contains(&format!("session {other}, files")),
        "a peer's file list is not in the brief"
    );
    assert!(!seat.prompt.contains("- P2 page:"));
    assert!(
        !seat.prompt.contains("- P10 docs:"),
        "a task with no seat yet is not listed"
    );
    let first = worker(&parsed(&complete()), &facts(), "P2", Uuid::from_u128(9)).unwrap();
    assert!(first.prompt.contains("- no other task has a seat yet"));
}

#[test]
fn every_brief_says_there_is_one_checkout_whatever_the_project_says() {
    let def = parsed(&complete());
    let mut prompts: Vec<String> = Vec::new();
    prompts.push(
        worker(&def, &facts(), "P2", Uuid::from_u128(9))
            .unwrap()
            .prompt,
    );
    prompts.push(
        template(&def, &facts(), "frontend-developer")
            .unwrap()
            .prompt,
    );
    for prompt in prompts {
        assert!(
            prompt.contains(
                "What this project's own instruction files say about worktrees does not apply in a fleet."
            ),
            "{prompt}"
        );
    }
}

#[test]
fn a_worker_brief_gives_tools_and_facts_and_no_working_method() {
    let def = parsed(&complete());
    let prompt = worker(&def, &facts(), "P2", Uuid::from_u128(9))
        .unwrap()
        .prompt;
    for fact in [
        "acknowledged by your next tool call",
        "Commit with fleet_commit",
        "The checkout's index is shared",
        "Report with fleet_report",
    ] {
        assert!(prompt.contains(fact), "{fact}: {prompt}");
    }
    for method in [
        "Then start the next slice",
        "Acknowledge each message",
        "formats a whole package",
        "stuck for ten minutes",
        "Before your first slice",
    ] {
        assert!(!prompt.contains(method), "{method}: {prompt}");
    }
}

#[test]
fn a_template_loads_the_rules_and_the_definition_and_stops() {
    let def = parsed(&complete());
    let seat = template(&def, &facts(), "frontend-developer").unwrap();
    assert_eq!(seat.seat, "template-frontend-developer");
    assert_eq!(seat.task, None);
    assert!(seat.prompt.contains("/srv/project/.claude/fleet/rules.md"));
    assert!(
        seat.prompt
            .contains("/srv/project/.claude/fleet/templates/frontend-developer.md")
    );
    assert!(
        seat.prompt
            .ends_with("End your turn with the single line READY.")
    );
    assert!(template(&def, &facts(), "rust-developer").is_none());
}

#[test]
fn the_sessions_mike_chose_are_read_back_from_the_definition() {
    let mut value = complete();
    assert_eq!(
        ControlSeats::recorded(&parsed(&value)),
        ControlSeats::answering(None, None)
    );
    super::definition::merge(&mut value, &control_patch(&seats()));
    assert_eq!(ControlSeats::recorded(&parsed(&value)), seats());
}

#[test]
fn recorded_done_does_not_release_dependent_tasks_without_evidence() {
    let def = changed(json!({"tasks": {"P1": {"state": "done"}, "P2": {"state": "working"}}}));
    let header = FleetHeader {
        fleet: "f".into(),
        slug: "s".into(),
        state: "running".into(),
        run_id: None,
        started_at: None,
        finished_at: None,
        now: chrono::Utc::now(),
        missing: vec![],
    };
    let status = build(header, &def, &[], &[]);
    assert!(passed_tasks(&status).is_empty());
}

#[test]
fn starts_names_the_control_seats_the_workers_that_start_at_once_and_the_tasks_that_wait() {
    let found = starts(&parsed(&complete()));
    assert_eq!(found.control_seats, 0);
    assert_eq!(found.workers, vec!["P1".to_string(), "P2".to_string()]);
    assert_eq!(found.seats, 3);
    assert_eq!(found.most_at_once, 3);
    assert_eq!(
        found.templates,
        vec!["db-dev".to_string(), "frontend-developer".to_string()]
    );
    assert!(found.reference);
    assert_eq!(found.waiting, vec!["P10".to_string()]);
}

#[test]
fn a_template_on_the_generic_worker_loads_the_list_named_for_its_discipline() {
    let def = changed(json!({"workers": {"frontend-developer": {"agent": "fleet-worker"}}}));
    let seat = template(&def, &facts(), "frontend-developer").unwrap();
    assert!(seat.prompt.contains(
        "/srv/project/.claude/fleet/templates/frontend-developer.md, if the file exists: follow its load list. If it does not, /srv/project/.claude/fleet/templates/fleet-worker.md, if that file exists."
    ));
}

fn twelve_task_fleet() -> serde_json::Value {
    let mut value = complete();
    let mut tasks = serde_json::Map::new();
    for n in 1..=12u128 {
        tasks.insert(
            format!("T{n}"),
            json!({
                "name": format!("task number {n}"),
                "discipline": "db-dev",
                "criteria": ["C1"],
                "files": [format!("crates/mike/src/area{n}/one.rs"), format!("crates/mike/src/area{n}/two.rs")],
                "brief": "Build the part and prove it with one run.",
                "seat": format!("db-dev-T{n}"),
                "session": Uuid::from_u128(100 + n).to_string()
            }),
        );
    }
    value["tasks"] = serde_json::Value::Object(tasks);
    value
}

#[test]
fn a_brief_for_twelve_tasks_is_under_two_thousand_tokens_and_names_every_peer() {
    let def = parsed(&twelve_task_fleet());
    let seat = worker(&def, &facts(), "T1", Uuid::from_u128(101)).unwrap();
    for n in 2..=12u128 {
        assert!(
            seat.prompt.contains(&Uuid::from_u128(100 + n).to_string()),
            "peer T{n} is not named"
        );
    }
    assert!(!seat.prompt.contains("area2/one.rs"));
    let tokens = seat.prompt.chars().count() as f64 / 3.5;
    assert!(tokens < 2000.0, "{tokens} tokens\n{}", seat.prompt);
}

#[test]
fn a_worker_hands_off_one_task_and_leaves_acceptance_to_the_durable_register() -> anyhow::Result<()>
{
    let def = parsed(&complete());
    let seat = worker(&def, &facts(), "P2", Uuid::from_u128(9))
        .ok_or_else(|| anyhow::anyhow!("worker plan missing for P2"))?;
    assert!(
        seat.prompt
            .contains("A passing floor and judge release dependent work.")
    );
    assert!(seat.prompt.contains("fleet_propose adds a task"));
    assert!(
        seat.instructions
            .contains("the session ends after its bounded handoff")
    );
    assert!(
        seat.instructions
            .contains("Readiness and acceptance are separate.")
    );
    assert!(!seat.prompt.contains("until the gate passes"));
    assert!(!seat.instructions.contains("until the gate passes"));
    Ok(())
}

#[test]
fn a_worker_reads_the_fleet_through_the_seat_view() {
    let def = parsed(&complete());
    let seat = worker(&def, &facts(), "P2", Uuid::from_u128(9)).unwrap();
    assert!(seat.prompt.contains("view \"seat\""));
    assert!(!seat.prompt.contains("/fleet/definition/tasks"));
    assert!(!seat.prompt.contains("[mike digest]"));
}

#[test]
fn a_worker_is_told_what_the_reference_seat_answers_and_how_to_reach_it() {
    let def = changed(json!({"reference": {"session": Uuid::from_u128(8).to_string()}}));
    let seat = worker(&def, &facts(), "P2", Uuid::from_u128(9)).unwrap();
    assert!(seat.prompt.contains(&format!(
        "The reference seat, session {}, answers where something is, how something works and whether anyone has hit a problem, with file and line: ask (to: reference)",
        Uuid::from_u128(8)
    )));
    assert!(!seat.prompt.contains("ten minutes"));
}

#[test]
fn a_worker_whose_contract_is_none_gets_no_appended_contract() {
    let def = changed(json!({"workers": {"frontend-developer": {"contract": "none"}}}));
    let seat = worker(&def, &facts(), "P2", Uuid::from_u128(9)).unwrap();
    assert_eq!(seat.contract, None);
    let args = super::seat::claude_args(&def, &seat);
    assert!(
        !args.iter().any(|arg| arg == "--append-system-prompt-file"),
        "{args:?}"
    );
}

#[test]
fn a_definition_without_staffing_adopts_no_unowned_files() {
    let def = parsed(&complete());
    assert!(!def.staffing.adopt_unowned);
    let opted_in = changed(json!({"staffing": {"adopt_unowned": true}}));
    assert!(opted_in.staffing.adopt_unowned);
}

#[test]
fn a_worker_with_no_agent_runs_plain_claude_code_with_the_worker_skill() {
    let def = changed(
        json!({"workers": {"frontend-developer": {"model": "sonnet", "agent": "", "contract": ""}}}),
    );
    assert!(
        super::checks::check_definition(&def)
            .iter()
            .all(|m| !m.item.starts_with("workers.frontend-developer"))
    );
    let seat = worker(&def, &facts(), "P2", Uuid::from_u128(9)).unwrap();
    assert_eq!(
        seat.contract.as_deref(),
        Some(super::definition::WORKER_SKILL_CONTRACT)
    );
    let args = super::seat::claude_args(&def, &seat);
    assert!(!args.iter().any(|arg| arg == "--agent"), "{args:?}");
    assert!(args.iter().any(|arg| arg == "--append-system-prompt-file"));
}
