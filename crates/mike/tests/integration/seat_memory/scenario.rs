use serde_json::json;
use sqlx::PgPool;

use super::fixture::{
    LaunchSpec, Seat, TurnSpec, event, launch, mail_turn, project, report_note, run, session,
    template, thread_entry, turn, verdict,
};

pub(crate) const TASK_ACCEPTED: &str = "T1";
pub(crate) const TASK_REJECTED: &str = "T2";

pub(crate) struct Scenario {
    pub(crate) project_id: i64,
    pub(crate) other_project_id: i64,
    pub(crate) run: String,
    pub(crate) worker: Seat,
    pub(crate) coordinator: Seat,
    pub(crate) peer: Seat,
}

struct Excluded {
    pool: PgPool,
    project_id: i64,
    run: String,
}

impl Excluded {
    async fn seat(&self, seat: &str, result: &str, marker: &str) -> anyhow::Result<Seat> {
        let s = session(&self.pool, self.project_id).await?;
        let spec = LaunchSpec {
            run: Some(&self.run),
            seat,
            brief: marker,
            result,
        };
        launch(&self.pool, &s, &spec).await?;
        let report = TurnSpec {
            opened_by: "operator",
            kind: "operator",
            session_class: "fleet_seat",
            user_prompt: marker,
            final_text: marker,
        };
        turn(&self.pool, self.project_id, &s, &report).await?;
        Ok(s)
    }

    async fn all(&self) -> anyhow::Result<()> {
        self.seat("observer-1", "success", "SM-EXCL-OBSERVER")
            .await?;
        self.seat("worker-f", "failed", "SM-EXCL-FAILED").await?;
        self.seat("template-x", "success", "SM-EXCL-TEMPLATE")
            .await?;
        let synthetic = self
            .seat("worker-s", "success", "SM-EXCL-SYNTHETIC")
            .await?;
        let probe = TurnSpec {
            opened_by: "operator",
            kind: "operator",
            session_class: "synthetic",
            user_prompt: "SM-EXCL-SYNTHETIC",
            final_text: "SM-EXCL-SYNTHETIC",
        };
        turn(&self.pool, self.project_id, &synthetic, &probe).await?;
        Ok(())
    }
}

async fn ungated_run(pool: &PgPool, project_id: i64) -> anyhow::Result<()> {
    let other_run = run(pool).await?;
    let s = session(pool, project_id).await?;
    let spec = LaunchSpec {
        run: Some(&other_run),
        seat: "ungated",
        brief: "SM-EXCL-UNGATED",
        result: "success",
    };
    launch(pool, &s, &spec).await
}

async fn other_project(pool: &PgPool) -> anyhow::Result<i64> {
    let project_id = project(pool).await?;
    let gated = run(pool).await?;
    let s = session(pool, project_id).await?;
    let spec = LaunchSpec {
        run: Some(&gated),
        seat: "elsewhere",
        brief: "SM-EXCL-OTHER-PROJECT",
        result: "success",
    };
    launch(pool, &s, &spec).await?;
    event(
        pool,
        &gated,
        Some(&s.session_uuid),
        Some("T9"),
        "report_filed",
        &json!({}),
    )
    .await?;
    Ok(project_id)
}

async fn included_seats(pool: &PgPool, project_id: i64, run: &str) -> anyhow::Result<[Seat; 3]> {
    let mut seats = Vec::new();
    for (name, brief) in [
        ("worker-a", "SM-BRIEF-WORKER"),
        ("coordinator", "SM-BRIEF-COORDINATOR"),
        ("worker-b", "SM-BRIEF-PEER"),
    ] {
        let s = session(pool, project_id).await?;
        let spec = LaunchSpec {
            run: Some(run),
            seat: name,
            brief,
            result: "success",
        };
        launch(pool, &s, &spec).await?;
        seats.push(s);
    }
    let [worker, coordinator, peer]: [Seat; 3] = seats
        .try_into()
        .map_err(|_| anyhow::anyhow!("three seats expected"))?;
    Ok([worker, coordinator, peer])
}

async fn template_seat(pool: &PgPool, project_id: i64, run: &str) -> anyhow::Result<()> {
    let s = session(pool, project_id).await?;
    let spec = LaunchSpec {
        run: Some(run),
        seat: "template-reporter",
        brief: "SM-BRIEF-TEMPLATE",
        result: "success",
    };
    launch(pool, &s, &spec).await?;
    template(pool, &s).await?;
    event(
        pool,
        run,
        Some(&s.session_uuid),
        Some("T3"),
        "report_filed",
        &json!({}),
    )
    .await
}

async fn reports(pool: &PgPool, project_id: i64, run: &str, worker: &Seat) -> anyhow::Result<()> {
    let note = report_note(pool, worker, "SM-REPORT-NOTE").await?;
    let payload = json!({"note_uuid": note, "criterion": "C1", "passed": true});
    event(
        pool,
        run,
        Some(&worker.session_uuid),
        Some(TASK_ACCEPTED),
        "report_filed",
        &payload,
    )
    .await?;
    let report = |kind, class, text| TurnSpec {
        opened_by: "operator",
        kind,
        session_class: class,
        user_prompt: "prompt",
        final_text: text,
    };
    for spec in [
        report("operator", "fleet_seat", "SM-REPORT-TURN"),
        report("task-notification", "fleet_seat", "SM-EXCL-NOTIFICATION"),
        report("loop-wakeup", "fleet_seat", "SM-EXCL-WAKEUP"),
        report("operator", "fleet_seat", "  "),
    ] {
        turn(pool, project_id, worker, &spec).await?;
    }
    thread_entry(pool, project_id, worker, "SM-THREAD-ENTRY").await
}

async fn mail(
    pool: &PgPool,
    project_id: i64,
    worker: &Seat,
    coordinator: &Seat,
    peer: &Seat,
) -> anyhow::Result<()> {
    for (from, body) in [
        (
            coordinator,
            "SM-MAIL-COORDINATOR rerun check 11 at 10:15 for the store change",
        ),
        (
            coordinator,
            "SM-MAIL-COORDINATOR rerun check 12 at 10:16 for the store change",
        ),
        (
            peer,
            "SM-MAIL-PEER the store change needs the new index first",
        ),
    ] {
        mail_turn(pool, project_id, from, worker, body).await?;
    }
    let observer = session(pool, project_id).await?;
    let spec = LaunchSpec {
        run: None,
        seat: "observer-2",
        brief: "SM-EXCL-OBSERVER-SENDER",
        result: "success",
    };
    launch(pool, &observer, &spec).await?;
    mail_turn(
        pool,
        project_id,
        &observer,
        worker,
        "SM-EXCL-OBSERVER-MAIL polling the seat",
    )
    .await?;
    let feedback = TurnSpec {
        opened_by: "machine",
        kind: "hook-feedback",
        session_class: "fleet_seat",
        user_prompt: "SM-FEEDBACK stop hook says the goal is not met",
        final_text: "",
    };
    turn(pool, project_id, worker, &feedback).await?;
    turn(pool, project_id, worker, &feedback).await?;
    Ok(())
}

async fn workflow_brief(pool: &PgPool, project_id: i64) -> anyhow::Result<()> {
    let s = session(pool, project_id).await?;
    let spec = LaunchSpec {
        run: None,
        seat: "workflow",
        brief: "SM-WORKFLOW-BRIEF",
        result: "success",
    };
    launch(pool, &s, &spec).await?;
    let work = TurnSpec {
        opened_by: "operator",
        kind: "operator",
        session_class: "fleet_seat",
        user_prompt: "prompt",
        final_text: "SM-WORKFLOW-REPORT",
    };
    turn(pool, project_id, &s, &work).await?;
    Ok(())
}

async fn verdicts(pool: &PgPool, run: &str) -> anyhow::Result<()> {
    verdict(pool, run, TASK_ACCEPTED, "C1", false).await?;
    verdict(pool, run, TASK_ACCEPTED, "C1", true).await?;
    verdict(pool, run, TASK_ACCEPTED, "C2", true).await?;
    verdict(pool, run, TASK_REJECTED, "C1", true).await?;
    verdict(pool, run, TASK_REJECTED, "C2", false).await
}

pub(crate) async fn build(pool: &PgPool) -> anyhow::Result<Scenario> {
    let project_id = project(pool).await?;
    let run = run(pool).await?;
    let [worker, coordinator, peer] = included_seats(pool, project_id, &run).await?;
    event(
        pool,
        &run,
        Some(&peer.session_uuid),
        Some(TASK_REJECTED),
        "report_filed",
        &json!({}),
    )
    .await?;
    reports(pool, project_id, &run, &worker).await?;
    mail(pool, project_id, &worker, &coordinator, &peer).await?;
    template_seat(pool, project_id, &run).await?;
    workflow_brief(pool, project_id).await?;
    verdicts(pool, &run).await?;
    let excluded = Excluded {
        pool: pool.clone(),
        project_id,
        run: run.clone(),
    };
    excluded.all().await?;
    ungated_run(pool, project_id).await?;
    let other_project_id = other_project(pool).await?;
    Ok(Scenario {
        project_id,
        other_project_id,
        run,
        worker,
        coordinator,
        peer,
    })
}
