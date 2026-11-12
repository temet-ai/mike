use serde_json::json;
use sqlx::PgPool;

use super::*;
use crate::launch::fleet::status::{SeatStatus, TaskStatus, Verdict};
use crate::launch::fleet::tests::changed;
use crate::store::persistence::change_record::test_seed::{self, ToolUse};
use crate::store::persistence::test_support::acquire_ephemeral_db;

async fn seed_seat_session(pool: &PgPool, project: i64, session: Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, NOW(), NOW())
         RETURNING id",
    )
    .bind(session)
    .bind(project)
    .fetch_one(pool)
    .await?)
}

async fn seed_read(
    pool: &PgPool,
    project: i64,
    (session_id, tool, input): (i64, &str, Value),
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let request_id = test_seed::request(pool, session_id, project, at).await?;
    let use_ = ToolUse {
        session_id,
        request_id,
        name: tool,
        file_path: None,
        at,
    };
    test_seed::tool_use_with_input(pool, &use_, &input).await?;
    Ok(())
}

fn seated_def(worker: Uuid, asked: Uuid) -> FleetDefinition {
    changed(json!({
        "repo": "/srv/project",
        "reference": {"enabled": true, "discipline": "rust-developer", "session": Uuid::new_v4().to_string()},
        "tasks": {
            "P1": {"seat": "db-dev-P1", "session": worker.to_string(), "files": ["crates/sim/src/"]},
            "P2": {"seat": "web-P2", "session": asked.to_string(), "files": ["frontend"]}
        }
    }))
}

#[tokio::test]
async fn recent_reads_with_seat_and_outside_uses_reads_only_unasked_seats_reading_tools()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let pool = persist.pool();
    let project = test_seed::project(pool).await?;
    let now = test_seed::micros_now()?;
    let (worker, asked, outsider) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let worker_id = seed_seat_session(pool, project, worker).await?;
    let asked_id = seed_seat_session(pool, project, asked).await?;
    let outsider_id = seed_seat_session(pool, project, outsider).await?;
    let b = json!({"file_path": "/srv/project/crates/client/src/b.rs"});
    let c = json!({"file_path": "/srv/project/crates/client/src/c.rs"});
    let d = json!({"file_path": "/srv/project/crates/client/src/d.rs"});
    let shell = json!({"command": "cat crates/client/src/b.rs"});
    seed_read(pool, project, (worker_id, "Read", b), now).await?;
    seed_read(pool, project, (worker_id, "Bash", shell), now).await?;
    seed_read(pool, project, (asked_id, "Read", c), now).await?;
    seed_read(pool, project, (outsider_id, "Read", d), now).await?;
    let stale = json!({"file_path": "/srv/project/crates/client/src/old.rs"});
    let earlier = now - Duration::minutes(30);
    seed_read(pool, project, (worker_id, "Read", stale), earlier).await?;

    let def = seated_def(worker, asked);
    let found = recent_reads(&persist, &def, now, |key| {
        key == "ask-reference-read:web-P2"
    })
    .await;
    assert_eq!(
        found,
        vec![Reading {
            seat: "db-dev-P1".into(),
            session: worker,
            tool: "Read".into(),
            path: Some("crates/client/src/b.rs".into()),
        }]
    );
    db.cleanup().await
}

#[tokio::test]
async fn recent_reads_when_the_database_is_closed_reads_nothing() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    persist.pool().close().await;
    let def = seated_def(Uuid::new_v4(), Uuid::new_v4());
    let found = recent_reads(&persist, &def, test_seed::micros_now()?, |_| false).await;
    assert!(found.is_empty(), "{found:?}");
    db.cleanup().await
}

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap_or_default()
}

fn def() -> FleetDefinition {
    changed(json!({
        "repo": "/srv/project",
        "reference": {"enabled": true, "discipline": "rust-developer", "session": Uuid::from_u128(40).to_string(), "quiet_minutes": 10},
        "tasks": {
            "P1": {"seat": "db-dev-P1", "session": Uuid::from_u128(11).to_string(), "files": ["crates/sim/src/"]},
            "P2": {"seat": "web-P2", "session": Uuid::from_u128(12).to_string(), "files": ["frontend"]}
        }
    }))
}

fn tool(session: u128, tool: &str, input: Value) -> ToolRead {
    ToolRead {
        session: Uuid::from_u128(session).to_string(),
        tool: tool.into(),
        input: Some(input),
    }
}

fn uses() -> Vec<ToolRead> {
    vec![
        tool(
            11,
            "Read",
            json!({"file_path": "/srv/project/crates/sim/src/a.rs"}),
        ),
        tool(
            11,
            "Read",
            json!({"file_path": "/srv/project/crates/client/src/b.rs"}),
        ),
        tool(
            11,
            "Read",
            json!({"file_path": "/srv/project/crates/client/src/c.rs"}),
        ),
        tool(
            11,
            "Read",
            json!({"file_path": "/srv/project/.claude/fleet/rules.md"}),
        ),
        tool(
            11,
            "Read",
            json!({"file_path": "/home/vimes/.claude/CLAUDE.md"}),
        ),
        tool(12, "Grep", json!({"pattern": "fn tick"})),
        tool(
            12,
            "Glob",
            json!({"pattern": "*.svelte", "path": "/srv/project/frontend"}),
        ),
        tool(11, "Bash", json!({"command": "cat crates/client/src/b.rs"})),
        tool(
            99,
            "Read",
            json!({"file_path": "/srv/project/crates/client/src/b.rs"}),
        ),
    ]
}

#[test]
fn a_worker_reading_outside_its_own_files_is_a_reading_and_its_own_files_are_not() {
    let found = readings(&def(), &uses());
    let seen: Vec<(&str, Option<&str>)> = found
        .iter()
        .map(|r| (r.seat.as_str(), r.path.as_deref()))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("db-dev-P1", Some("crates/client/src/b.rs")),
            ("db-dev-P1", Some("crates/client/src/c.rs")),
            ("web-P2", None),
        ]
    );
}

#[test]
fn each_reading_worker_is_mailed_one_line_naming_the_reference_seat_and_what_it_holds() {
    let found = readings(&def(), &uses());
    let told = on_reading(&def(), &found, |_| false);
    assert_eq!(told.len(), 2, "{told:?}");
    assert_eq!(told[0].to, Uuid::from_u128(11));
    assert_eq!(told[0].key, "ask-reference-read:db-dev-P1");
    assert!(
        told[0].body.starts_with(&format!(
            "[mike reference] You are reading `crates/client/src/b.rs`, outside your task's files: the reference seat (session {}) holds what every worker started with (the run's rules, the rust-developer template's reading",
            Uuid::from_u128(40)
        )),
        "{}",
        told[0].body
    );
    assert!(!told[0].body.contains('\n'));
    assert!(told[1].body.contains("You are searching the whole tree"));
    let once = on_reading(&def(), &found, |key| key == "ask-reference-read:db-dev-P1");
    assert_eq!(once.len(), 1);
    let off = changed(json!({"reference": {"enabled": false}}));
    assert!(on_reading(&off, &found, |_| false).is_empty());
}

fn task(id: &str, seat: &str) -> TaskStatus {
    TaskStatus {
        id: id.into(),
        name: format!("{id} work"),
        discipline: "db-dev".into(),
        criteria: vec![],
        seat: Some(seat.into()),
        last_seat: None,
        state: "working".into(),
        passes: 0,
        fails: 0,
        rounds: 0,
        last: None,
        passed_criteria: vec![],
        ready: false,
    }
}

fn seat(name: &str, opened: &str) -> SeatStatus {
    SeatStatus {
        seat: name.into(),
        role: "db-dev".into(),
        model: "sonnet".into(),
        state: "working".into(),
        session: Some(Uuid::from_u128(if name == "db-dev-P1" { 11 } else { 12 })),
        goal: None,
        context_tokens: None,
        requests: 3,
        last_request_at: None,
        opened_at: Some(at(opened)),
        closed: false,
        first_request_cache: None,
    }
}

fn status() -> FleetStatus {
    FleetStatus {
        fleet: "f".into(),
        slug: "s".into(),
        state: "running".into(),
        run_id: Some("fleet-s-1".into()),
        goal: String::new(),
        started_at: Some(at("2026-10-01T12:40:00Z")),
        finished_at: None,
        now: at("2026-10-01T13:00:00Z"),
        missing: vec![],
        criteria: vec![],
        tasks: vec![task("P1", "db-dev-P1"), task("P2", "web-P2")],
        seats: vec![
            seat("db-dev-P1", "2026-10-01T12:44:00Z"),
            seat("web-P2", "2026-10-01T12:55:00Z"),
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

fn verdict(at_: &str, verdict: &str, task: &str) -> Verdict {
    Verdict {
        at: at(at_),
        verdict: verdict.into(),
        title: format!("{task} slice"),
        tasks: vec![task.into()],
        criteria: vec![],
    }
}

#[test]
fn a_worker_whose_slice_failed_is_told_to_ask_the_reference_seat() {
    let mut failed = status();
    failed.verdicts = vec![
        verdict("2026-10-01T12:58:00Z", "fail", "P1"),
        verdict("2026-10-01T12:59:00Z", "pass", "P2"),
        verdict("2026-10-01T12:41:00Z", "fail", "P2"),
    ];
    let told = on_a_fail(&def(), &failed);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].to, Uuid::from_u128(11));
    assert!(told[0].key.starts_with("ask-reference-fail:") && told[0].key.ends_with(":P1"));
    assert!(told[0].body.starts_with(
        "[mike reference] Acceptance failed your slice of P1 at 12:58:00Z: mike has sent the failure to the reference seat"
    ));
}

#[test]
fn a_worker_ten_minutes_in_without_a_commit_is_told_to_ask_the_reference_seat() {
    let told = on_no_commit(&def(), &status(), &BTreeSet::new());
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].to, Uuid::from_u128(11));
    assert_eq!(told[0].key, "ask-reference-quiet:db-dev-P1");
    assert!(
        told[0]
            .body
            .contains("You have worked 16 minutes on P1 without a commit")
    );
    let committed = BTreeSet::from(["db-dev-P1".to_string()]);
    assert!(on_no_commit(&def(), &status(), &committed).is_empty());
}
