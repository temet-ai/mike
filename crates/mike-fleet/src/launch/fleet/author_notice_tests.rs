use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{items, mail_text, tell};
use crate::launch::fleet::status::{FleetStatus, ProblemStatus};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap_or_default()
}

fn row(author: Option<Uuid>, run: &str) -> FleetRow {
    FleetRow {
        id: 1,
        fleet_uuid: Uuid::new_v4(),
        slug: "night-run".into(),
        project_path: "/repo".into(),
        state: "running".into(),
        definition: serde_json::json!({}),
        missing: serde_json::json!([]),
        run_id: Some(run.into()),
        start_report: None,
        created_by: author,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        ready_at: Some(Utc::now()),
        started_at: None,
        finished_at: None,
    }
}

fn status(problems: Vec<ProblemStatus>, idle: Option<i64>) -> FleetStatus {
    FleetStatus {
        fleet: "f".into(),
        slug: "night-run".into(),
        state: "running".into(),
        run_id: Some("run".into()),
        goal: String::new(),
        started_at: Some(at("2026-11-14T09:00:00Z")),
        finished_at: None,
        now: at("2026-11-14T10:42:00Z"),
        missing: vec![],
        criteria: vec![],
        tasks: vec![],
        seats: vec![],
        verdicts: vec![],
        blocked: vec![],
        needs_operator: vec![],
        accepted: false,
        obligations: vec![],
        every_seat_idle_seconds: idle,
        tree_broken: None,
        problems,
    }
}

fn problem(escalated: bool) -> ProblemStatus {
    ProblemStatus {
        trigger: "criterion_failed_thrice".into(),
        task: Some("T3".into()),
        opened_at: at("2026-11-14T10:00:00Z"),
        detail: "C2 failed three times".into(),
        decisions: vec![],
        escalated,
        routed: false,
    }
}

#[test]
fn an_escalated_problem_names_the_action_that_settles_it() {
    let row = row(Some(Uuid::new_v4()), "run");
    let found = items(
        &row,
        &status(vec![problem(true), problem(false)], None),
        &[],
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].line.contains("criterion_failed_thrice on task T3"));
    assert!(found[0].line.contains("define_fleet fleet="));
    assert!(found[0].line.contains("mike fleet stop night-run"));
}

#[test]
fn a_fleet_idle_ten_minutes_is_reported_and_one_idle_five_is_not() {
    let row = row(None, "run");
    let idle = items(&row, &status(vec![], Some(900)), &[]);
    assert_eq!(idle.len(), 1);
    assert!(idle[0].line.contains("idle for 15 min"));
    assert!(items(&row, &status(vec![], Some(300)), &[]).is_empty());
    let later = items(&row, &status(vec![], Some(960)), &[]);
    assert_eq!(later[0].key, idle[0].key);
}

#[test]
fn the_mail_lists_every_fresh_item() {
    let row = row(None, "run");
    let found = items(&row, &status(vec![problem(true)], Some(900)), &[]);
    let text = mail_text(&row, &found);
    assert!(
        text.starts_with("[mike] Fleet night-run needs you"),
        "{text}"
    );
    assert_eq!(text.matches("\n- ").count(), 2, "{text}");
}

#[tokio::test]
async fn the_author_is_told_each_unresolved_item_once() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let author = Uuid::new_v4();
    persist
        .launch()
        .ensure_session_exists(&author.to_string())
        .await?;
    let run = format!("fleet-author-{}", Uuid::new_v4());
    let row = row(Some(author), &run);
    let stuck = status(vec![problem(true)], None);
    tell(&persist, &row, &stuck).await;
    tell(&persist, &row, &stuck).await;
    let inbox = persist
        .launch()
        .fetch_for_session_with_activity(author)
        .await?
        .inbox;
    assert_eq!(
        inbox.len(),
        1,
        "{:?}",
        inbox.iter().map(|m| &m.row.body).collect::<Vec<_>>()
    );
    assert!(inbox[0].row.body.contains("criterion_failed_thrice"));
    let nobody = row_without_author(&run);
    tell(&persist, &nobody, &stuck).await;
    db.cleanup().await
}

fn row_without_author(run: &str) -> FleetRow {
    row(None, run)
}
