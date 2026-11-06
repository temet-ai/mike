use super::tests::{at, empty, seat, task, verdict};
use crate::launch::fleet::status::{Blocked, CriterionStatus, FleetStatus};
use chrono::Duration;

pub(super) fn fixture() -> anyhow::Result<FleetStatus> {
    let now = at("2026-09-29T01:30:00Z")?;
    let pass = verdict(
        "2026-09-29T00:05:00Z",
        "pass",
        "C1 P1 schema landed (d6318ed1)",
        "P1",
    )?;
    let fail = verdict(
        "2026-09-29T01:12:00Z",
        "fail",
        "C2 P2 b39d9351: main does not compile; read_point, rollup, change_log gaps",
        "P2",
    )?;
    let mut tasks = vec![
        task("P1", "schema", "db-dev", "passed"),
        task("P2", "approve+feed", "rust-developer", "rework"),
        task("P3", "docs", "frontend-developer", "working"),
        task("P4", "design audit", "ui-designer", "queued"),
    ];
    tasks[0].passes = 1;
    tasks[0].fails = 1;
    tasks[0].last = Some(pass.clone());
    tasks[0].seat = Some("db-dev-P1".to_string());
    tasks[1].fails = 2;
    tasks[1].last = Some(fail.clone());
    tasks[1].seat = Some("rust-developer-P2".to_string());
    tasks[1].criteria = vec!["C2".to_string(), "C4".to_string()];
    tasks[2].seat = Some("frontend-developer-P3".to_string());
    let mut seats = vec![
        seat("acceptance", "acceptance", "idle", Some(525_000), false),
        seat(
            "rust-developer-P2",
            "rust-developer",
            "idle",
            Some(946_000),
            false,
        ),
        seat("observer-9", "observer", "working", None, false),
        seat(
            "ui-designer-P4",
            "ui-designer",
            "silent",
            Some(693_000),
            false,
        ),
        seat("db-dev-P1", "db-dev", "closed", Some(100_000), true),
    ];
    seats[0].last_request_at = Some(now - Duration::seconds(120));
    Ok(FleetStatus {
        state: "running".to_string(),
        run_id: Some("fleet-fleet-workspace-3f0a1c52".to_string()),
        started_at: Some(now - Duration::minutes(363)),
        criteria: vec![
            CriterionStatus {
                id: "C1".to_string(),
                text: "one".to_string(),
                tasks: 2,
                ready: 0,
                passed: 1,
            },
            CriterionStatus {
                id: "C2".to_string(),
                text: "two".to_string(),
                tasks: 1,
                ready: 0,
                passed: 0,
            },
            CriterionStatus {
                id: "C3".to_string(),
                text: "three".to_string(),
                tasks: 1,
                ready: 0,
                passed: 1,
            },
        ],
        tasks,
        seats,
        verdicts: vec![
            fail,
            pass,
            verdict(
                "2026-09-28T23:09:00Z",
                "fail",
                "C1 P3 4 of 7 tests fail",
                "P3",
            )?,
        ],
        blocked: vec![Blocked {
            seat: "frontend-developer-P3".to_string(),
            title: "P3 progress: improvement-area frontend".to_string(),
        }],
        ..empty(now)
    })
}
