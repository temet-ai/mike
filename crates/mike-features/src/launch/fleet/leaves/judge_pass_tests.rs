#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::judge_pass::{
    JUDGES_PER_TICK, Pending, judge_pending, pending, report_data, report_from,
};
use super::tests::Canned;
use super::{LeafError, LeafModel};
use crate::launch::fleet::commit_tests::scratch_repo;
use crate::launch::fleet::events::{
    EventKind, EventPayload, FleetEvent, NewFleetEvent, ReportFiled, SeatLaunchConfirmed,
};
use crate::launch::fleet::tests::{complete, parsed};
use crate::launch::fleet::watch::git;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::session_notes::NewSessionNote;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_running,
};

const RUN: &str = "fleet-judge-pass";

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_default()
}

fn event(id: i64, task: &str, kind: EventKind, payload: Value, seconds: i64) -> FleetEvent {
    FleetEvent {
        id,
        run_id: RUN.into(),
        task: Some(task.into()),
        session_uuid: Some(Uuid::from_u128(7)),
        kind,
        payload,
        occurred_at: at(seconds),
    }
}

fn report(id: i64, task: &str, note: u128, seconds: i64) -> FleetEvent {
    event(
        id,
        task,
        EventKind::ReportFiled,
        json!({"note_uuid": Uuid::from_u128(note)}),
        seconds,
    )
}

fn verdict(id: i64, task: &str, note: u128, seconds: i64) -> FleetEvent {
    event(
        id,
        task,
        EventKind::VerdictRecorded,
        json!({"note_uuid": Uuid::from_u128(note), "criterion": "C1", "passed": true}),
        seconds,
    )
}

fn tasks(found: &[Pending]) -> Vec<&str> {
    found.iter().map(|p| p.task.as_str()).collect()
}

#[test]
fn the_newest_unjudged_report_of_a_task_is_pending_once_its_floor_passed() {
    let mut value = complete();
    value["tasks"]["P10"]["done_when"] =
        json!({"checks": [{"operation": "check", "expected_exit": 0}]});
    let def = parsed(&value);
    let events = [
        report(1, "P1", 1, 10),
        report(2, "P2", 2, 11),
        verdict(3, "P2", 2, 12),
        report(4, "P10", 3, 13),
    ];
    assert_eq!(
        tasks(&pending(&def, &events)),
        vec!["P1"],
        "P2 is judged, P10 waits for its floor"
    );
    let floored = [
        &events[..],
        &[event(
            5,
            "P10",
            EventKind::FloorPassed,
            json!({"sha": "abc"}),
            14,
        )],
    ]
    .concat();
    assert_eq!(tasks(&pending(&def, &floored)), vec!["P1", "P10"]);
    let failed_floor = [
        &events[..],
        &[event(
            5,
            "P10",
            EventKind::FloorFailed,
            json!({"sha": "abc", "check": "x"}),
            14,
        )],
    ]
    .concat();
    assert_eq!(tasks(&pending(&def, &failed_floor)), vec!["P1"]);
    let re_reported = [
        &events[..],
        &[verdict(5, "P1", 1, 15), report(6, "P1", 4, 16)],
    ]
    .concat();
    let found = pending(&def, &re_reported);
    assert_eq!(tasks(&found), vec!["P1"]);
    assert_eq!(
        found[0].note,
        Uuid::from_u128(4),
        "the verdict was for the older report"
    );
    assert_eq!(JUDGES_PER_TICK, 2);
}

struct Refusing;

#[async_trait]
impl LeafModel for Refusing {
    async fn answer(&self, _system: &str, _user: &str, _schema: Value) -> Result<Value, LeafError> {
        Err(LeafError::Refused(
            "OpenRouter call rate limit exceeded (30/min)".into(),
        ))
    }
}

struct Fixture {
    db: TestDbHandle,
    persist: PersistHandle,
    def: crate::launch::fleet::definition::FleetDefinition,
    seat: Uuid,
    run: String,
    fleet: Uuid,
    note: Uuid,
    sha: String,
    _dir: tempfile::TempDir,
}

async fn fixture() -> anyhow::Result<Option<Fixture>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (dir, repo) = scratch_repo().await?;
    let sha = git(&repo, &["rev-parse", "HEAD"], None)
        .await
        .map(|raw| String::from_utf8_lossy(&raw).trim().to_string())
        .ok_or_else(|| anyhow::anyhow!("no head"))?;
    let seat = Uuid::new_v4();
    persist
        .launch()
        .ensure_session_exists(&seat.to_string())
        .await?;
    let mut value = complete();
    value["repo"] = json!(repo);
    let def = parsed(&value);
    let run = format!("{RUN}-{}", Uuid::new_v4());
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: run.clone(),
            project_path: Some(repo.clone()),
            backend: "none".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let fleet = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: run.clone(),
            project_path: repo,
            definition: serde_json::to_value(&def)?,
            missing: json!([]),
            created_by: None,
        })
        .await?
        .fleet_uuid;
    seed_fleet_running(&persist, fleet, &run).await?;
    let tags = vec![
        format!("fleet:{fleet}"),
        "report".to_string(),
        "task:P1".to_string(),
    ];
    let data = json!({
        "task": "P1", "criteria": ["C1"], "commit": sha,
        "exercised": {"command": "true", "exit": 0, "output_ref": "receipt 1"},
        "tests_changed": ["mine/a.txt"], "unestablished": []
    });
    let note = persist
        .launch()
        .create_session_note(
            seat,
            NewSessionNote {
                kind: Some("report"),
                title: "Report P1: the base",
                body: Some("the base commit"),
                status: None,
                tags: Some(&tags),
                data: Some(&data),
            },
        )
        .await?
        .note_uuid;
    for payload in [
        EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed { launch_id: 1 }),
        EventPayload::ReportFiled(ReportFiled { note_uuid: note }),
    ] {
        persist
            .fleet()
            .append_fleet_event(&NewFleetEvent {
                run_id: run.clone(),
                task: Some("P1".into()),
                session_uuid: Some(seat),
                payload,
            })
            .await?;
    }
    Ok(Some(Fixture {
        db,
        persist,
        def,
        seat,
        run,
        fleet,
        note,
        sha,
        _dir: dir,
    }))
}

impl Fixture {
    async fn events(&self) -> anyhow::Result<Vec<FleetEvent>> {
        Ok(self
            .persist
            .fleet()
            .fleet_events_since(&self.run, 0)
            .await?)
    }

    async fn verdicts(&self) -> anyhow::Result<usize> {
        Ok(self
            .events()
            .await?
            .iter()
            .filter(|e| e.kind == EventKind::VerdictRecorded)
            .count())
    }

    async fn inbox(&self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .persist
            .launch()
            .fetch_for_session_with_activity(self.seat)
            .await?
            .inbox
            .into_iter()
            .map(|m| m.row.body)
            .collect())
    }

    async fn judge(&self, model: &dyn LeafModel) -> anyhow::Result<Vec<Pending>> {
        let found = pending(&self.def, &self.events().await?);
        judge_pending(
            model,
            &self.persist,
            (&self.def, &self.run, &self.fleet.to_string()),
            &found,
        )
        .await;
        Ok(found)
    }
}

#[tokio::test]
async fn the_pass_judges_a_filed_report_from_its_note_and_then_has_nothing_pending()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let found = pending(&f.def, &f.events().await?);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].note, f.note);
    let data = report_data(&f.persist, &found[0])
        .await
        .ok_or_else(|| anyhow::anyhow!("the report note is in the seat's notebook"))?;
    let built = report_from(&data, &found[0], (&f.run, "fleet")).unwrap();
    assert_eq!(
        (built.task.as_str(), built.sha.as_str()),
        ("P1", f.sha.as_str())
    );
    assert_eq!(built.criteria, vec!["C1"]);
    assert_eq!(built.exercised.as_ref().map(|e| e.exit), Some(Some(0)));
    let answer = json!({
        "verdict": "pass", "criteria": ["C1"], "sha": f.sha, "evidence_read": ["mine/a.txt"],
        "needed": null, "observation": "the base commit carries mine/a.txt"
    });
    let model = Canned::new(vec![answer]);
    assert_eq!(f.judge(&model).await?.len(), 1);
    assert_eq!(f.verdicts().await?, 1);
    assert!(
        f.inbox()
            .await?
            .iter()
            .any(|b| b.starts_with("[mike judge] PASS task P1"))
    );
    assert!(
        f.judge(&model).await?.is_empty(),
        "a judged report is not judged again"
    );
    assert_eq!(model.calls(), 1);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_refused_call_records_nothing_says_nothing_and_leaves_the_report_pending()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert_eq!(f.judge(&Refusing).await?.len(), 1);
    assert_eq!(f.verdicts().await?, 0);
    assert!(f.inbox().await?.is_empty());
    assert_eq!(
        f.judge(&Refusing).await?.len(),
        1,
        "still pending for the next tick"
    );
    f.db.cleanup().await?;
    Ok(())
}
