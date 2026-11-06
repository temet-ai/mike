use chrono::{DateTime, TimeZone, Utc};

use super::attention::{Clock, assess};
use super::ledger::{GitOutputRow, WorkItemRow, build_ledger};

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 23, hour, minute, 0)
        .single()
        .unwrap_or_default()
}

fn row(kind: &str, task_id: &str, state: &str, owner: Option<&str>) -> WorkItemRow {
    WorkItemRow {
        kind: kind.into(),
        task_id: task_id.into(),
        description: Some(format!("{task_id} work")),
        owner_agent_id: owner.map(str::to_string),
        state: state.into(),
        launched_at: at(10, 0),
        finished_at: None,
        exit_code: None,
        summary: None,
        last_request_at: Some(at(10, 5)),
        request_count: 3,
        last_edit_at: None,
    }
}

fn kinds(rows: &[WorkItemRow], git: &[GitOutputRow], clock: &Clock) -> Vec<&'static str> {
    let ledger = build_ledger("s", rows, git);
    let mut kinds: Vec<_> = assess(&ledger, clock).into_iter().map(|a| a.kind).collect();
    kinds.sort_unstable();
    kinds
}

#[test]
fn orphaned_agent_is_a_lost_result() {
    let rows = vec![
        row("agent", "parent", "completed", None),
        row("agent", "child", "running", Some("parent")),
    ];
    let clock = Clock {
        now: at(10, 6),
        main_last_request_at: Some(at(10, 6)),
    };
    assert_eq!(kinds(&rows, &[], &clock), vec!["result_lost"]);
}

#[test]
fn background_job_whose_owner_went_quiet_is_a_stalled_wait() {
    let rows = vec![
        row("agent", "worker", "running", None),
        row("bash", "bbuild", "running", Some("worker")),
    ];
    let clock = Clock {
        now: at(10, 30),
        main_last_request_at: Some(at(10, 29)),
    };
    assert_eq!(
        kinds(&rows, &[], &clock),
        vec!["agent_silent", "stalled_wait"]
    );
}

#[test]
fn recent_activity_raises_nothing() {
    let rows = vec![
        row("agent", "worker", "running", None),
        row("bash", "bbuild", "running", Some("worker")),
    ];
    let clock = Clock {
        now: at(10, 10),
        main_last_request_at: Some(at(10, 9)),
    };
    assert!(kinds(&rows, &[], &clock).is_empty());
}

#[test]
fn idle_main_thread_with_open_work_is_raised_once_per_idle_period() {
    let rows = vec![row("bash", "bdeploy", "running", None)];
    let clock = Clock {
        now: at(11, 0),
        main_last_request_at: Some(at(10, 20)),
    };
    let ledger = build_ledger("s", &rows, &[]);
    let found = assess(&ledger, &clock);
    let idle = found.iter().find(|a| a.kind == "main_idle_with_open_work");
    assert!(idle.is_some_and(|a| a.subject.starts_with("main@")));
    assert!(found.iter().any(|a| a.kind == "stalled_wait"));
}

#[test]
fn main_idle_is_not_raised_while_an_open_agent_is_still_making_requests() {
    let mut worker = row("agent", "worker", "running", None);
    worker.last_request_at = Some(at(10, 55));
    let clock = Clock {
        now: at(11, 0),
        main_last_request_at: Some(at(10, 20)),
    };
    assert!(!kinds(&[worker], &[], &clock).contains(&"main_idle_with_open_work"));
}

#[test]
fn main_idle_is_raised_when_every_open_agent_is_silent() {
    let mut worker = row("agent", "worker", "running", None);
    worker.last_request_at = Some(at(10, 21));
    let clock = Clock {
        now: at(11, 0),
        main_last_request_at: Some(at(10, 20)),
    };
    assert!(kinds(&[worker], &[], &clock).contains(&"main_idle_with_open_work"));
}

#[test]
fn main_idle_subject_changes_with_the_open_count_so_a_stale_notice_is_withdrawn() {
    let clock = Clock {
        now: at(11, 0),
        main_last_request_at: Some(at(10, 20)),
    };
    let subject = |rows: &[WorkItemRow]| {
        assess(&build_ledger("s", rows, &[]), &clock)
            .into_iter()
            .find(|a| a.kind == "main_idle_with_open_work")
            .map(|a| a.subject)
    };
    let one = vec![row("bash", "b1", "running", None)];
    let two = vec![
        row("bash", "b1", "running", None),
        row("bash", "b2", "running", None),
    ];
    assert_ne!(subject(&one), subject(&two));
}

#[test]
fn agent_that_pushed_and_only_polls_is_flagged() {
    let mut worker = row("agent", "worker", "running", None);
    worker.last_edit_at = Some(at(10, 1));
    worker.last_request_at = Some(at(10, 25));
    let git = vec![GitOutputRow {
        agent_id: Some("worker".into()),
        at: at(10, 2),
        text: "   1111111..2222222  main -> main".into(),
    }];
    let clock = Clock {
        now: at(10, 26),
        main_last_request_at: Some(at(10, 26)),
    };
    assert_eq!(kinds(&[worker], &git, &clock), vec!["landed_polling"]);
}

#[test]
fn finished_work_raises_nothing() {
    let rows = vec![
        row("agent", "worker", "completed", None),
        row("bash", "bdone", "completed", Some("worker")),
    ];
    let clock = Clock {
        now: at(12, 0),
        main_last_request_at: Some(at(10, 0)),
    };
    assert!(kinds(&rows, &[], &clock).is_empty());
}
