#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::events::{EventKind, FleetEvent};

use super::propose::{BY_MIKE, Judged, judge, plan};
use super::propose_tests::{applied, proposal};
use super::propose_tick::{
    coordinator_mail, nearest, proposals_for, quiet_unowned_changes, suspect_criteria,
    unowned_failures,
};
use super::tests::{complete, parsed};
use crate::store::persistence::fleet::{ChangedPathRow, TreeCheckRow};

fn changed_at(path: &str, minutes_ago: i64, now: DateTime<Utc>) -> ChangedPathRow {
    ChangedPathRow {
        path: path.into(),
        seen_at: now - Duration::minutes(minutes_ago),
    }
}

#[test]
fn mike_proposes_for_an_unowned_change_only_once_it_has_been_quiet_ten_minutes_and_only_once() {
    let now = Utc::now();
    let def = parsed(&complete());
    let rows = vec![
        changed_at("scripts/leftover.sh", 30, now),
        changed_at("scripts/busy.sh", 30, now),
        changed_at("scripts/busy.sh", 2, now),
        changed_at("frontend/src/routes/improvement/a.svelte", 30, now),
        changed_at("crates/mike/src/store/persistence/mod.rs", 30, now),
    ];
    assert_eq!(
        quiet_unowned_changes(&def, &rows, now),
        vec!["scripts/leftover.sh"]
    );
    let proposed = applied(
        &def,
        plan(
            &def,
            &proposal(&[], &["x"]),
            Judged::Clean,
            (BY_MIKE, &["changed:scripts/leftover.sh".to_string()]),
        )
        .patch,
    );
    assert!(
        quiet_unowned_changes(&proposed, &rows, now).is_empty(),
        "one proposal per cause"
    );
}

fn failed_check(output: &str) -> TreeCheckRow {
    TreeCheckRow {
        id: 4,
        run_id: "run".into(),
        state: "abc".into(),
        state_after: None,
        command: "cargo check".into(),
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        exit_code: Some(101),
        output: output.into(),
        askers: Vec::new(),
        told: Vec::new(),
    }
}

#[test]
fn mike_proposes_for_a_check_failing_in_files_no_task_owns_and_only_once() {
    let def = parsed(&complete());
    let output = "error[E0425]: cannot find value\n  --> scripts/tool/main.rs:4:9\n  --> frontend/src/routes/improvement/a.ts:1:1\n";
    assert_eq!(
        unowned_failures(&def, &failed_check(output)),
        vec!["scripts/tool/main.rs"]
    );
    let mut passed = failed_check(output);
    passed.exit_code = Some(0);
    assert!(unowned_failures(&def, &passed).is_empty());
    let known = applied(
        &def,
        plan(
            &def,
            &proposal(&[], &["x"]),
            Judged::Clean,
            (BY_MIKE, &["failing:scripts/tool/main.rs".to_string()]),
        )
        .patch,
    );
    assert!(unowned_failures(&known, &failed_check(output)).is_empty());
}

#[test]
fn mikes_proposal_takes_the_discipline_of_the_nearest_owned_neighbour_and_lists_a_gone_file_as_new()
{
    let def = parsed(&complete());
    assert_eq!(
        nearest(&def, "frontend/src/routes/runs/+page.svelte"),
        ("frontend-developer".to_string(), vec!["C2".to_string()])
    );
    assert_eq!(
        nearest(&def, "scripts/x.sh"),
        ("db-dev".to_string(), vec!["C1".to_string()])
    );
    let exists = |p: &str| p != "frontend/src/gone.ts";
    let made = proposals_for(
        &def,
        "changed",
        &["frontend/src/a.ts".into(), "frontend/src/gone.ts".into()],
        &exists,
    );
    assert_eq!(made.len(), 1);
    let (p, causes) = &made[0];
    assert_eq!(p.files, vec!["frontend/src/a.ts"]);
    assert_eq!(p.new_files, vec!["frontend/src/gone.ts"]);
    assert_eq!(
        causes,
        &vec![
            "changed:frontend/src/a.ts".to_string(),
            "changed:frontend/src/gone.ts".to_string()
        ]
    );
    assert_eq!(judge(&def, p, &exists), Judged::Clean);
    let answer = plan(&def, p, Judged::Clean, (BY_MIKE, causes)).answer;
    let mail = coordinator_mail(p, &answer);
    assert!(
        mail.contains("added it as task P11") && mail.contains("proposal-1"),
        "{mail}"
    );
}

fn fleet_event(kind: EventKind, payload: Value) -> FleetEvent {
    FleetEvent {
        id: 1,
        run_id: "run".into(),
        task: None,
        session_uuid: None,
        kind,
        payload,
        occurred_at: Utc::now(),
    }
}

fn verdict(criterion: &str, passed: bool) -> FleetEvent {
    fleet_event(
        EventKind::VerdictRecorded,
        json!({"note_uuid": Uuid::nil(), "criterion": criterion, "passed": passed}),
    )
}

#[test]
fn a_criterion_that_fails_three_times_is_suspect_once_and_passes_do_not_count() {
    let two = vec![
        verdict("C6", false),
        verdict("C6", true),
        verdict("C6", false),
        verdict("C7", true),
    ];
    assert!(suspect_criteria(&two).is_empty());
    let three = [two, vec![verdict("C6", false)]].concat();
    assert_eq!(suspect_criteria(&three), vec![("C6".to_owned(), 3)]);
    let marked = [
        three,
        vec![fleet_event(
            EventKind::CriterionSuspect,
            json!({"criterion": "C6", "failures": 3}),
        )],
    ]
    .concat();
    assert!(suspect_criteria(&marked).is_empty());
}
