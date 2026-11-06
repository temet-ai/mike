#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::report_judge::{ReportToJudge, judge_report};
use super::tests::Canned;
use crate::launch::fleet::commit_tests::scratch_repo;
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::events::{
    EventKind, EventPayload, FleetEvent, NewFleetEvent, ReportFiled, SeatLaunchConfirmed,
};
use crate::launch::fleet::floor::Outcome;
use crate::launch::fleet::report::Exercised;
use crate::launch::fleet::status::{FleetHeader, FleetStatus, build_from_record};
use crate::launch::fleet::tests::{complete, parsed};
use crate::launch::fleet::watch::git;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_running,
};

struct Fixture {
    db: TestDbHandle,
    persist: PersistHandle,
    def: FleetDefinition,
    seat: Uuid,
    run: String,
    fleet: Uuid,
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
    persist.ensure_session_exists(&seat.to_string()).await?;
    let mut value = complete();
    value["repo"] = json!(repo);
    let def = parsed(&value);
    let run = format!("fleet-judge-{}", Uuid::new_v4());
    persist
        .upsert_run(&NewRun {
            run_id: run.clone(),
            project_path: Some(repo.clone()),
            backend: "none".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let fleet = persist
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
    for payload in [
        EventPayload::SeatLaunchConfirmed(SeatLaunchConfirmed { launch_id: 1 }),
        EventPayload::ReportFiled(ReportFiled {
            note_uuid: Uuid::new_v4(),
        }),
    ] {
        persist
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
        sha,
        _dir: dir,
    }))
}

impl Fixture {
    fn report(&self) -> ReportToJudge {
        ReportToJudge {
            run: self.run.clone(),
            fleet: self.fleet.to_string(),
            task: "P1".into(),
            seat: self.seat,
            note: Uuid::new_v4(),
            sha: self.sha.clone(),
            criteria: vec!["C1".into()],
            exercised: Some(Exercised {
                command: "true".into(),
                exit: Some(0),
                output_ref: "receipt 1".into(),
            }),
            tests_changed: vec!["mine/a.txt".into()],
            unestablished: Vec::new(),
        }
    }

    fn answer(&self, verdict: &str, needed: Value) -> Value {
        json!({
            "verdict": verdict,
            "criteria": ["C1"],
            "sha": self.sha,
            "evidence_read": ["mine/a.txt"],
            "needed": needed,
            "observation": "the base commit carries mine/a.txt"
        })
    }

    async fn verdicts(&self) -> anyhow::Result<Vec<FleetEvent>> {
        Ok(self
            .persist
            .fleet_events_since(&self.run, 0)
            .await?
            .into_iter()
            .filter(|event| event.kind == EventKind::VerdictRecorded)
            .collect())
    }

    async fn inbox(&self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .persist
            .fetch_for_session_with_activity(self.seat)
            .await?
            .inbox
            .into_iter()
            .map(|m| m.row.body)
            .collect())
    }

    async fn status(&self) -> anyhow::Result<FleetStatus> {
        let events = self.persist.fleet_events_since(&self.run, 0).await?;
        Ok(build_from_record(
            FleetHeader {
                fleet: self.fleet.to_string(),
                slug: self.run.clone(),
                state: "running".into(),
                run_id: Some(self.run.clone()),
                started_at: Some(Utc::now()),
                finished_at: None,
                now: Utc::now(),
                missing: vec![],
            },
            &self.def,
            &[],
            &[],
            &events,
        ))
    }
}

#[tokio::test]
async fn a_passing_judge_verdict_is_recorded_against_the_report_and_accepts_the_task()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let report = f.report();
    let model = Canned::new(vec![f.answer("pass", Value::Null)]);
    let floor = Outcome::Passed { sha: f.sha.clone() };
    judge_report(&model, &f.persist, &f.def, &report, floor).await?;
    let asked = model.user(0);
    assert!(
        asked.contains("mine/a.txt") && asked.contains("\"task\":\"P1\""),
        "{asked}"
    );
    let verdicts = f.verdicts().await?;
    assert_eq!(verdicts.len(), 1, "{verdicts:?}");
    assert_eq!(verdicts[0].task.as_deref(), Some("P1"));
    assert_eq!(verdicts[0].session_uuid, None);
    assert_eq!(verdicts[0].payload["criterion"], "C1");
    assert_eq!(verdicts[0].payload["passed"], true);
    assert_eq!(verdicts[0].payload["note_uuid"], json!(report.note));
    let mail = f.inbox().await?;
    assert!(
        mail.iter()
            .any(|body| body.starts_with("[mike judge] PASS task P1 C1")),
        "{mail:?}"
    );
    let status = f.status().await?;
    let p1 = status.tasks.iter().find(|t| t.id == "P1").unwrap();
    assert_eq!((p1.state.as_str(), p1.passes, p1.fails), ("passed", 1, 0));
    assert!(p1.ready);
    assert_eq!(status.criteria[0].passed, 1);
    assert!(!status.accepted, "C2 has no verdict yet");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_failing_judge_verdict_sends_the_task_to_rework_with_what_is_needed() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let model = Canned::new(vec![f.answer("fail", json!("a test for the empty string"))]);
    let floor = Outcome::Passed { sha: f.sha.clone() };
    judge_report(&model, &f.persist, &f.def, &f.report(), floor).await?;
    let verdicts = f.verdicts().await?;
    assert_eq!(verdicts.len(), 1, "{verdicts:?}");
    assert_eq!(verdicts[0].payload["passed"], false);
    assert_eq!(verdicts[0].payload["needed"], "a test for the empty string");
    let mail = f.inbox().await?;
    assert!(
        mail.iter().any(|body| {
            body.starts_with("[mike judge] FAIL task P1 C1")
                && body.contains("Needed before a pass: a test for the empty string")
        }),
        "{mail:?}"
    );
    let status = f.status().await?;
    let p1 = status.tasks.iter().find(|t| t.id == "P1").unwrap();
    assert_eq!((p1.state.as_str(), p1.passes, p1.fails), ("rework", 0, 1));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_judge_that_cannot_answer_records_no_verdict_and_returns_the_error() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let model = Canned::new(Vec::new());
    let floor = Outcome::Passed { sha: f.sha.clone() };
    assert!(
        judge_report(&model, &f.persist, &f.def, &f.report(), floor)
            .await
            .is_err()
    );
    assert!(f.verdicts().await?.is_empty());
    assert!(
        f.inbox().await?.is_empty(),
        "the pass retries; the seat hears of a verdict or a give-up"
    );
    let status = f.status().await?;
    let p1 = status.tasks.iter().find(|t| t.id == "P1").unwrap();
    assert_eq!(p1.state, "awaiting acceptance");
    f.db.cleanup().await?;
    Ok(())
}
