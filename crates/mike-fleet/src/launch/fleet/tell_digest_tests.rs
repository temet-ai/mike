use std::collections::BTreeSet;

use super::*;
use crate::launch::fleet::status::{SeatStatus, TaskStatus};

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap_or_default()
}

fn task(id: &str, seat: &str, passes: usize, fails: usize) -> TaskStatus {
    TaskStatus {
        id: id.into(),
        name: format!("{id} work"),
        discipline: "sim".into(),
        criteria: vec![],
        seat: Some(seat.into()),
        last_seat: None,
        state: "working".into(),
        passes,
        fails,
        rounds: 0,
        last: None,
        passed_criteria: vec![],
        ready: false,
    }
}

fn seat(name: &str, state: &str, last: &str) -> SeatStatus {
    SeatStatus {
        seat: name.into(),
        role: name.into(),
        model: "sonnet".into(),
        state: state.into(),
        session: None,
        goal: None,
        context_tokens: None,
        requests: 3,
        last_request_at: Some(at(last)),
        opened_at: Some(at("2026-10-04T09:00:00Z")),
        closed: false,
        first_request_cache: None,
    }
}

fn status() -> FleetStatus {
    FleetStatus {
        fleet: "f".into(),
        slug: "core-game-4".into(),
        state: "running".into(),
        run_id: Some("fleet-core-game-4-1".into()),
        goal: String::new(),
        started_at: Some(at("2026-10-04T09:00:00Z")),
        finished_at: None,
        now: at("2026-10-04T10:42:00Z"),
        missing: vec![],
        criteria: vec![],
        tasks: vec![task("T1", "sim-T1", 1, 0), task("T2", "sim-T2", 0, 2)],
        seats: vec![
            seat("sim-T1", "working", "2026-10-04T10:41:30Z"),
            seat("sim-T2", "idle", "2026-10-04T10:30:00Z"),
            seat("coordinator", "idle", "2026-10-04T10:20:00Z"),
            seat("acceptance", "working", "2026-10-04T10:41:50Z"),
        ],
        verdicts: vec![],
        blocked: vec![],
        needs_operator: vec![],
        accepted: false,
        obligations: vec![],
        every_seat_idle_seconds: None,
        tree_broken: None,
        problems: Vec::new(),
    }
}

fn control() -> ControlSeats {
    ControlSeats {
        coordinator: Uuid::from_u128(1),
        acceptance: Uuid::from_u128(2),
        observer: Some(Uuid::from_u128(3)),
    }
}

fn queue() -> SlotQueue {
    SlotQueue {
        waiting: 3,
        slots: 2,
    }
}

fn told(sent: &[Notice]) -> BTreeSet<String> {
    sent.iter()
        .flat_map(|n| std::iter::once(n.key.clone()).chain(n.covers.clone()))
        .filter(|k| !k.starts_with("digest-at:"))
        .collect()
}

fn first(status: &FleetStatus, waits: Waits) -> Vec<Notice> {
    digests(status, &control(), (queue(), waits), |_| false)
}

#[test]
fn the_first_digest_names_each_task_its_seat_idle_time_passes_fails_and_the_slot_queue() {
    let sent = first(&status(), Waits::default());
    let to: Vec<Uuid> = sent.iter().map(|n| n.to).collect();
    assert_eq!(
        to,
        vec![
            control().coordinator,
            control().acceptance,
            Uuid::from_u128(3)
        ]
    );
    let text = &sent[0].body;
    assert!(text.starts_with("[mike digest] Fleet core-game-4 at 10:42:00Z."));
    assert!(
        text.contains("- T1 (sim-T1): working, ready false, accepted false, 1 pass, 0 fails\n"),
        "{text}"
    );
    assert!(
        text.contains("- T2 (sim-T2): working, idle since 10:30:00Z, ready false, accepted false, 0 passes, 2 fails"),
        "{text}"
    );
    assert!(text.contains("Builds waiting for a slot: 3 (2 slots)."));
}

#[test]
fn the_same_state_is_not_sent_again() {
    let told = told(&first(&status(), Waits::default()));
    assert!(
        digests(&status(), &control(), (queue(), Waits::default()), |k| told
            .contains(k))
        .is_empty()
    );
}

#[test]
fn idle_drift_and_a_longer_queue_send_nothing_even_to_a_working_seat() {
    let told = told(&first(&status(), Waits::default()));
    let mut drift = status();
    drift.seats[0].last_request_at = Some(at("2026-10-04T10:35:00Z"));
    drift.seats[1].last_request_at = Some(at("2026-10-04T10:10:00Z"));
    drift.every_seat_idle_seconds = Some(900);
    let longer = SlotQueue {
        waiting: 7,
        slots: 2,
    };
    let sent = digests(&drift, &control(), (longer, Waits::default()), |k| {
        told.contains(k)
    });
    assert!(sent.is_empty(), "{sent:?}");
}

#[test]
fn a_verdict_sends_one_mail_per_control_seat_with_only_the_lines_that_changed() {
    let told = told(&first(&status(), Waits::default()));
    let mut verdict = status();
    verdict.tasks[0].passes = 2;
    verdict.seats[1].last_request_at = Some(at("2026-10-04T10:10:00Z"));
    let sent = digests(&verdict, &control(), (queue(), Waits::default()), |k| {
        told.contains(k)
    });
    assert_eq!(sent.len(), 3, "a new pass goes to every control seat");
    for notice in &sent {
        assert!(
            notice.body.contains("- T1 (sim-T1): working"),
            "{}",
            notice.body
        );
        assert!(notice.body.contains("2 passes"), "{}", notice.body);
        assert!(!notice.body.contains("- T2"), "{}", notice.body);
        assert_eq!(notice.body.matches("\n- ").count(), 1, "{}", notice.body);
    }
}

#[test]
fn a_readiness_or_state_change_alone_sends_that_rows_line() {
    let told = told(&first(&status(), Waits::default()));
    let send = |changed: &FleetStatus| {
        digests(changed, &control(), (queue(), Waits::default()), |k| {
            told.contains(k)
        })
    };
    let mut ready = status();
    ready.tasks[0].ready = true;
    let sent = send(&ready);
    assert_eq!(sent.len(), 3);
    assert!(
        sent[0].body.contains("- T1 (sim-T1): working, ready true"),
        "{}",
        sent[0].body
    );
    assert!(!sent[0].body.contains("- T2"), "{}", sent[0].body);
    let mut moved = status();
    moved.tasks[1].state = "awaiting acceptance".into();
    let sent = send(&moved);
    assert_eq!(sent.len(), 3);
    assert!(
        sent[0].body.contains("- T2 (sim-T2): awaiting acceptance"),
        "{}",
        sent[0].body
    );
}

#[test]
fn a_task_getting_accepted_or_a_new_obligation_is_sent() {
    let told = told(&first(&status(), Waits::default()));
    let mut accepted = status();
    accepted.tasks[1].criteria = vec!["C1".into()];
    accepted.tasks[1].passed_criteria = vec!["C1".into()];
    let sent = digests(&accepted, &control(), (queue(), Waits::default()), |k| {
        told.contains(k)
    });
    assert_eq!(sent.len(), 3);
    assert!(
        sent[0]
            .body
            .contains("- T2 (sim-T2): working, idle since 10:30:00Z, ready false, accepted true")
    );
}

fn waiting(task: &str, seat: &str, since: &str, newest: u128) -> Waiting {
    Waiting {
        task: task.into(),
        seat: seat.into(),
        since: at(since),
        reports: 1,
        newest: Uuid::from_u128(newest),
        newest_at: at(since),
    }
}

#[test]
fn a_report_is_listed_once_it_has_waited_ten_minutes_and_not_before() {
    let young = vec![waiting("T1", "sim-T1", "2026-10-04T10:40:00Z", 8)];
    let old = vec![waiting("T2", "sim-T2", "2026-10-04T10:30:00Z", 7)];
    let before = told(&first(&status(), Waits::default()));
    let sent = |reports: &[Waiting]| {
        digests(
            &status(),
            &control(),
            (
                queue(),
                Waits {
                    reports,
                    questions: &[],
                },
            ),
            |k| before.contains(k),
        )
    };
    assert!(sent(&young).is_empty(), "a two minute wait sends nothing");
    let late = sent(&old);
    assert_eq!(late.len(), 3);
    assert!(
        late[0]
            .body
            .contains("Reports waiting for a verdict, oldest first: T2 (sim-T2) 12 min, 1 report."),
        "{}",
        late[0].body
    );
    let again: BTreeSet<String> = before.iter().cloned().chain(told(&late)).collect();
    let repeat = digests(
        &status(),
        &control(),
        (
            queue(),
            Waits {
                reports: &old,
                questions: &[],
            },
        ),
        |k| again.contains(k),
    );
    assert!(repeat.is_empty(), "the same wait is not sent twice");
}

#[test]
fn a_blocked_question_is_listed_and_pushed_once() {
    let questions = vec![(
        Uuid::from_u128(70),
        "db-dev-T5 (session 1) has waited 7 minutes on task T3 (question 70): which resolver?"
            .to_string(),
    )];
    let with = Waits {
        reports: &[],
        questions: &questions,
    };
    let told = told(&first(&status(), Waits::default()));
    let pushed = digests(&status(), &control(), (queue(), with), |k| told.contains(k));
    assert_eq!(
        pushed.len(),
        3,
        "a blocked question goes to every control seat"
    );
    assert!(
        pushed[0].body.contains("Blocked question open five minutes or more: db-dev-T5 (session 1) has waited 7 minutes on task T3 (question 70): which resolver?"),
        "{}",
        pushed[0].body
    );
}

#[test]
fn the_queue_counts_the_wrapped_commands_past_the_definitions_slot_count() {
    let open = |id: i64, minute: u32| TreeResultRow {
        use_id: id,
        seat: Some("sim-T1".into()),
        session_uuid: None,
        command: Some(format!("scripts/cargo-slot cargo test -p p{id}")),
        started_at: Some(at(&format!("2026-10-04T10:{minute:02}:00Z"))),
        finished_at: None,
        is_error: None,
        content: None,
    };
    let commands = vec![open(1, 1), open(2, 2), open(3, 3), open(4, 4)];
    let fleet = |capacity: u32| FleetDefinition {
        wrappers: vec!["scripts/cargo-slot".into()],
        execution: serde_json::from_value(serde_json::json!({"timed_commands": {"cargo": {
            "resources": [{"name": "build", "capacity": capacity}]
        }}}))
        .unwrap_or_default(),
        ..FleetDefinition::default()
    };
    assert_eq!(
        queue_of(&fleet(2), &commands),
        SlotQueue {
            waiting: 2,
            slots: 2
        }
    );
    assert_eq!(
        queue_of(&fleet(3), &commands),
        SlotQueue {
            waiting: 1,
            slots: 3
        }
    );
    assert_eq!(
        queue_of(&FleetDefinition::default(), &commands),
        SlotQueue::default()
    );
}

#[test]
fn a_digest_is_not_sent_twice_inside_one_three_minute_window() {
    let first = digests(&status(), &control(), (queue(), Waits::default()), |_| {
        false
    });
    let recent: BTreeSet<String> = first
        .iter()
        .flat_map(|n| n.covers.clone())
        .filter(|k| k.starts_with("digest-at:"))
        .collect();
    let mut verdict = status();
    verdict.tasks[1].fails = 3;
    assert!(
        digests(&verdict, &control(), (queue(), Waits::default()), |k| {
            recent.contains(k)
        })
        .is_empty()
    );
}
