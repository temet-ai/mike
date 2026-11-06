#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use super::definition::{FleetDefinition, merge};
use super::propose::{
    ADDED, BY_MIKE, HELD, Judged, Proposal, REFUSED, answer_or_refusal, judge, judge_done,
    next_task_id, owner_question, plan, question_recipients, task_value,
};
use super::propose_tick::settle_patch;
use super::staff::{Hold, warm_order};
use super::tests::{changed, complete, parsed};
use crate::store::persistence::fleet::FleetSeatRow;

pub(super) fn proposal(files: &[&str], new_files: &[&str]) -> Proposal {
    Proposal {
        name: "the run record page".into(),
        why: "acceptance needs to read the run record".into(),
        files: files.iter().map(|f| (*f).to_string()).collect(),
        new_files: new_files.iter().map(|f| (*f).to_string()).collect(),
        criteria: vec!["C2".into()],
        after: Vec::new(),
        discipline: "frontend-developer".into(),
    }
}

fn everything_exists(_: &str) -> bool {
    true
}

fn nothing_exists(_: &str) -> bool {
    false
}

#[test]
fn a_proposal_whose_files_exist_and_nobody_owns_is_clean() {
    let def = parsed(&complete());
    let p = proposal(
        &["frontend/src/routes/runs/"],
        &["frontend/src/routes/runs/+page.svelte"],
    );
    assert_eq!(judge(&def, &p, &everything_exists), Judged::Clean);
}

#[test]
fn a_file_that_does_not_exist_is_refused_unless_it_is_listed_as_new() {
    let def = parsed(&complete());
    let Judged::Refused(problems) = judge(
        &def,
        &proposal(&["frontend/src/runs.ts"], &[]),
        &nothing_exists,
    ) else {
        panic!("a missing file must be refused");
    };
    assert!(problems[0].contains("frontend/src/runs.ts") && problems[0].contains("new_files"));
    let new_only = proposal(&[], &["frontend/src/runs.ts"]);
    assert_eq!(judge(&def, &new_only, &nothing_exists), Judged::Clean);
}

#[test]
fn an_unknown_criterion_discipline_or_task_is_refused_with_what_exists() {
    let def = parsed(&complete());
    let mut p = proposal(&[], &["frontend/src/runs.ts"]);
    p.criteria = vec!["C9".into()];
    p.discipline = "rust-developer".into();
    p.after = vec!["P7".into()];
    let Judged::Refused(problems) = judge(&def, &p, &everything_exists) else {
        panic!("every broken rule is reported");
    };
    let all = problems.join("\n");
    assert!(
        all.contains("`C9` is not a criterion") && all.contains("`C1`, `C2`"),
        "{all}"
    );
    assert!(
        all.contains("`rust-developer` has no workers") && all.contains("`db-dev`"),
        "{all}"
    );
    assert!(all.contains("`P7` in `after` is not a task"), "{all}");
}

#[test]
fn a_proposal_with_no_name_files_or_criteria_is_refused() {
    let def = parsed(&complete());
    let empty = Proposal {
        discipline: "db-dev".into(),
        ..Proposal::default()
    };
    let Judged::Refused(problems) = judge(&def, &empty, &everything_exists) else {
        panic!("an empty proposal is refused");
    };
    assert_eq!(problems.len(), 3, "{problems:?}");
}

#[test]
fn a_file_an_open_task_owns_holds_the_proposal_as_a_question_to_that_task() {
    let def = parsed(&complete());
    let p = proposal(&["frontend/src/routes/improvement/runs.svelte"], &[]);
    let Judged::Overlap(owners) = judge(&def, &p, &everything_exists) else {
        panic!("an owned file is a question, never a refusal");
    };
    assert_eq!(
        owners.get("P2").cloned(),
        Some(vec![
            "frontend/src/routes/improvement/runs.svelte".to_string()
        ])
    );
    let folder = proposal(&["frontend/src/routes/"], &[]);
    assert!(matches!(
        judge(&def, &folder, &everything_exists),
        Judged::Overlap(_)
    ));
}

#[test]
fn a_finished_task_releases_a_proposal_but_shared_files_still_need_a_producer_or_order() {
    let def = parsed(&complete());
    let p = proposal(&["frontend/src/routes/improvement/runs.svelte"], &[]);
    let done = std::collections::BTreeSet::from(["P2".to_string()]);
    assert_eq!(
        judge_done(&def, &p, &everything_exists, &done),
        Judged::Clean
    );
    assert!(matches!(
        judge(&def, &p, &everything_exists),
        Judged::Overlap(_)
    ));
    let shared = proposal(&["crates/mike/src/store/persistence/mod.rs"], &[]);
    assert!(matches!(
        judge(&parsed(&complete()), &shared, &everything_exists),
        Judged::Overlap(_)
    ));
}

#[test]
fn the_next_task_takes_the_fleets_own_prefix_and_the_next_free_number() {
    assert_eq!(next_task_id(&parsed(&complete())), "P11");
    assert_eq!(next_task_id(&FleetDefinition::default()), "T1");
}

pub(super) fn applied(def: &FleetDefinition, patch: Option<serde_json::Value>) -> FleetDefinition {
    let mut value = serde_json::to_value(def).unwrap();
    if let Some(patch) = patch {
        merge(&mut value, &patch);
    }
    parsed(&value)
}

#[test]
fn a_clean_proposal_adds_a_task_the_staffing_tick_starts_with_a_brief_from_name_why_and_files() {
    let def = parsed(&complete());
    let p = proposal(
        &["frontend/src/routes/runs/"],
        &["frontend/src/routes/runs/+page.svelte"],
    );
    let planned = plan(
        &def,
        &p,
        Judged::Clean,
        ("6c8c2d1e-0000-4000-8000-000000000009", &[]),
    );
    assert_eq!(planned.answer.state, ADDED);
    assert_eq!(planned.answer.task.as_deref(), Some("P11"));
    let after = applied(&def, planned.patch);
    let task = after.tasks.get("P11").expect("the task is added");
    assert_eq!(task.discipline, "frontend-developer");
    assert!(
        task.files
            .contains(&"frontend/src/routes/runs/+page.svelte".to_string())
    );
    for words in [
        "the run record page",
        "acceptance needs to read the run record",
        "frontend/src/routes/runs/",
    ] {
        assert!(
            task.brief.contains(words),
            "{words} missing from {}",
            task.brief
        );
    }
    assert!(task.seat.is_none(), "the staffing tick gives it a seat");
    let ready = super::plan::ready_tasks(&after, &std::collections::BTreeSet::new());
    assert!(ready.contains(&"P11".to_string()));
    assert_eq!(
        after.proposals.get("proposal-1").map(|r| r.state.as_str()),
        Some(ADDED)
    );
}

#[test]
fn a_held_proposal_is_recorded_without_a_task_and_a_seats_refusal_is_not_recorded() {
    let def = parsed(&complete());
    let p = proposal(&["frontend/src/routes/improvement/x.svelte"], &[]);
    let owners = [(
        "P2".to_string(),
        vec!["frontend/src/routes/improvement/x.svelte".to_string()],
    )]
    .into_iter()
    .collect();
    let held = plan(&def, &p, Judged::Overlap(owners), ("seat", &[]));
    assert_eq!(held.answer.state, HELD);
    let after = applied(&def, held.patch);
    assert_eq!(after.tasks.len(), 3);
    assert_eq!(
        after.proposals.get("proposal-1").map(|r| r.owners.clone()),
        Some(vec!["P2".to_string()])
    );
    let refused = plan(&def, &p, Judged::Refused(vec!["no".into()]), ("seat", &[]));
    assert!(refused.patch.is_none() && refused.answer.proposal.is_none());
    let by_mike = plan(
        &def,
        &p,
        Judged::Refused(vec!["no".into()]),
        (BY_MIKE, &["changed:x".to_string()]),
    );
    let kept = applied(&def, by_mike.patch);
    assert_eq!(
        kept.proposals.get("proposal-1").map(|r| r.state.as_str()),
        Some(REFUSED)
    );
}

#[test]
fn a_held_proposal_is_added_once_its_owner_releases_the_files_and_left_alone_when_declined() {
    let def = parsed(&complete());
    let p = proposal(&["frontend/src/routes/improvement/x.svelte"], &[]);
    let owners = [("P2".to_string(), vec![])].into_iter().collect();
    let held = applied(
        &def,
        plan(&def, &p, Judged::Overlap(owners), ("seat", &[])).patch,
    );
    let (still, added) = settle_patch(&held, &everything_exists);
    assert!(
        still.is_none() && added.is_empty(),
        "the owner has not answered"
    );
    let mut released = serde_json::to_value(&held).unwrap();
    merge(
        &mut released,
        &json!({"tasks": {"P2": {"files": ["frontend/src/routes/other/**"]}}}),
    );
    let released = parsed(&released);
    let (patch, added) = settle_patch(&released, &everything_exists);
    assert_eq!(added, vec![("proposal-1".to_string(), "P11".to_string())]);
    let after = applied(&released, patch);
    assert!(after.tasks.contains_key("P11"));
    let mut declined = serde_json::to_value(&released).unwrap();
    merge(
        &mut declined,
        &json!({"proposals": {"proposal-1": {"state": "declined"}}}),
    );
    assert_eq!(
        settle_patch(&parsed(&declined), &everything_exists),
        (None, Vec::new())
    );
    let gone = settle_patch(&held, &nothing_exists).0;
    assert!(
        gone.is_some_and(|p| p.to_string().contains("\"refused\"")),
        "a held proposal whose file has gone is refused"
    );
}

#[test]
fn an_overlap_is_asked_of_the_owning_seat_or_the_coordinator_and_answered_as_held_not_refused() {
    let owner = Uuid::from_u128(22);
    let coordinator = Uuid::from_u128(1);
    let proposer = Uuid::from_u128(9);
    let def = changed(json!({
        "coordinator": {"session": coordinator},
        "tasks": {"P2": {"seat": "frontend-developer-P2", "session": owner}}
    }));
    let p = proposal(
        &[
            "frontend/src/routes/improvement/x.svelte",
            "crates/mike/src/store/persistence/improvement/a.rs",
        ],
        &[],
    );
    let judged = judge(&def, &p, &everything_exists);
    let answer = plan(&def, &p, judged, (&proposer.to_string(), &[])).answer;
    assert_eq!(answer.state, HELD);
    let (asker, owners) = question_recipients(&def, &proposer.to_string(), &answer).unwrap();
    assert_eq!(asker, proposer);
    let to: Vec<(&str, Option<Uuid>)> = owners.iter().map(|o| (o.task.as_str(), o.to)).collect();
    assert_eq!(
        to,
        vec![("P1", Some(coordinator)), ("P2", Some(owner))],
        "P2's own seat is asked; P1 has no seat, so its question goes to the coordinator"
    );
    assert_eq!(
        owners[1].files,
        vec!["frontend/src/routes/improvement/x.svelte".to_string()]
    );
    let (by_mike, _) = question_recipients(&def, BY_MIKE, &answer).unwrap();
    assert_eq!(
        by_mike, coordinator,
        "mike's own proposal asks in the coordinator's name"
    );
    let text = owner_question(
        Uuid::nil(),
        "mike",
        &p,
        ("P2", &owners[1].files),
        "proposal-1",
    );
    assert!(
        text.contains("answer or decline") && text.contains("\"state\": \"declined\""),
        "{text}"
    );
    let held = answer_or_refusal(answer).expect("a held proposal is an ordinary answer");
    assert_eq!(held.state, HELD);
    let refused = plan(
        &def,
        &p,
        Judged::Refused(vec!["`x` does not exist".into()]),
        ("s", &[]),
    )
    .answer;
    assert!(answer_or_refusal(refused).is_err_and(|e| e.contains("`x` does not exist")));
}

fn three_forks() -> FleetDefinition {
    changed(json!({"tasks": {
        "P3": {"name": "a", "discipline": "frontend-developer", "criteria": ["C2"], "files": ["frontend/a/"], "brief": "a"},
        "P4": {"name": "b", "discipline": "frontend-developer", "criteria": ["C2"], "files": ["frontend/b/"], "brief": "b"}
    }}))
}

fn seat_row(session: Uuid, answered_at: Option<DateTime<Utc>>) -> FleetSeatRow {
    FleetSeatRow {
        seat: Some("frontend-developer-P2".into()),
        role: Some("frontend-developer".into()),
        agent: None,
        model: None,
        session_uuid: Some(session),
        surface_ref: None,
        opened_at: Some(Utc::now()),
        closed_at: None,
        requests: i64::from(answered_at.is_some()),
        last_request_at: answered_at,
        context_tokens: None,
        last_status: answered_at.map(|_| 200),
        forked: true,
        first_cache_read: None,
        first_cache_written: None,
    }
}

#[test]
fn of_three_forks_due_at_once_one_starts_and_the_two_wait_for_its_first_answer() {
    let now = Utc::now();
    let def = three_forks();
    let ready = vec!["P1".to_string(), "P2".into(), "P3".into(), "P4".into()];
    let (start, held) = warm_order(&def, &ready, &[], now);
    assert_eq!(start, vec!["P1", "P2"], "a single db-dev fork is unchanged");
    assert_eq!(
        held,
        vec![
            Hold {
                task: "P3".into(),
                waiting_for: "frontend-developer-P2".into()
            },
            Hold {
                task: "P4".into(),
                waiting_for: "frontend-developer-P2".into()
            },
        ]
    );
    let session = Uuid::from_u128(42);
    let first = {
        let mut v = serde_json::to_value(&def).unwrap();
        merge(
            &mut v,
            &json!({"tasks": {"P2": {"seat": "frontend-developer-P2", "session": session, "state": "working"}}}),
        );
        parsed(&v)
    };
    let waiting = vec!["P3".to_string(), "P4".into()];
    let (start, held) = warm_order(&first, &waiting, &[seat_row(session, None)], now);
    assert!(
        start.is_empty() && held.len() == 2,
        "the first fork has not answered yet"
    );
    let (start, held) = warm_order(&first, &waiting, &[seat_row(session, Some(now))], now);
    assert_eq!(
        (start, held),
        (vec!["P3".to_string(), "P4".into()], Vec::new())
    );
}

#[test]
fn a_proposed_task_waits_for_nothing_it_did_not_name() {
    let value = task_value(&proposal(&["a"], &[]), "mike");
    assert_eq!(value["after"], json!([]));
}
