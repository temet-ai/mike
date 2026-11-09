mod fixture;
mod scenario;
mod search;

use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::test_db::TestDb;
use fixture::{LaunchSpec, event, launch, session};
use scenario::{Scenario, TASK_ACCEPTED, TASK_REJECTED, build};

#[derive(sqlx::FromRow)]
struct Row {
    item_kind: String,
    session_uuid: String,
    run_id: Option<String>,
    seat: Option<String>,
    task: Option<String>,
    author: String,
    from_session: Option<String>,
    from_role: Option<String>,
    text: String,
    repeat_count: i32,
    report_filed: bool,
    verdict_accepted: bool,
}

async fn rows(pool: &PgPool, project_id: i64) -> anyhow::Result<Vec<Row>> {
    Ok(sqlx::query_as(
        "SELECT item_kind, session_uuid::text AS session_uuid, run_id, seat, task, author,
                from_session::text AS from_session, from_role, text, repeat_count,
                report_filed, verdict_accepted
           FROM memory.seat_memory($1)",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?)
}

async fn with_scenario<F, Fut>(name: &str, check: F) -> anyhow::Result<()>
where
    F: FnOnce(PgPool, Scenario) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping {name}");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;
    let scenario = build(&pool).await?;
    let outcome = check(pool.clone(), scenario).await;
    pool.close().await;
    db.cleanup().await?;
    outcome
}

fn kinds(rows: &[Row], kind: &str) -> Vec<String> {
    let mut texts: Vec<String> = rows
        .iter()
        .filter(|r| r.item_kind == kind)
        .map(|r| r.text.clone())
        .collect();
    texts.sort();
    texts
}

fn row<'a>(rows: &'a [Row], kind: &str, marker: &str) -> anyhow::Result<&'a Row> {
    rows.iter()
        .find(|r| r.item_kind == kind && r.text.contains(marker))
        .ok_or_else(|| anyhow::anyhow!("no {kind} row containing {marker}"))
}

#[tokio::test]
async fn seat_memory_returns_one_row_of_each_included_kind() -> anyhow::Result<()> {
    with_scenario(
        "seat_memory_returns_one_row_of_each_included_kind",
        |pool, s| async move {
            let rows = rows(&pool, s.project_id).await?;
            assert_eq!(kinds(&rows, "seat_brief").len(), 4);
            assert_eq!(kinds(&rows, "workflow_brief"), ["SM-WORKFLOW-BRIEF"]);
            assert_eq!(
                kinds(&rows, "seat_report"),
                ["SM-REPORT-NOTE", "SM-REPORT-TURN"]
            );
            assert_eq!(kinds(&rows, "seat_mail").len(), 1);
            assert_eq!(kinds(&rows, "peer_message").len(), 1);
            assert_eq!(kinds(&rows, "seat_goal_feedback").len(), 1);
            assert_eq!(kinds(&rows, "seat_thread_entry"), ["SM-THREAD-ENTRY"]);
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn seat_memory_leaves_out_every_excluded_kind() -> anyhow::Result<()> {
    with_scenario(
        "seat_memory_leaves_out_every_excluded_kind",
        |pool, s| async move {
            let rows = rows(&pool, s.project_id).await?;
            let leaked: Vec<&str> = rows
                .iter()
                .filter(|r| r.text.contains("SM-EXCL"))
                .map(|r| r.text.as_str())
                .collect();
            assert!(
                leaked.is_empty(),
                "excluded material reached search: {leaked:?}"
            );
            assert!(rows.iter().all(|r| !r.text.trim().is_empty()));
            let seats: Vec<&str> = rows.iter().filter_map(|r| r.seat.as_deref()).collect();
            for gone in [
                "observer-1",
                "worker-f",
                "template-x",
                "worker-s",
                "ungated",
            ] {
                assert!(!seats.contains(&gone), "seat {gone} must not appear");
            }
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn seat_memory_reads_one_project_only() -> anyhow::Result<()> {
    with_scenario("seat_memory_reads_one_project_only", |pool, s| async move {
        let other = rows(&pool, s.other_project_id).await?;
        assert_eq!(kinds(&other, "seat_brief"), ["SM-EXCL-OTHER-PROJECT"]);
        assert_eq!(other.len(), 1);
        let own = rows(&pool, s.project_id).await?;
        assert!(own.iter().all(|r| !r.text.contains("OTHER-PROJECT")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn seat_memory_collapses_repeats_and_counts_them() -> anyhow::Result<()> {
    with_scenario(
        "seat_memory_collapses_repeats_and_counts_them",
        |pool, s| async move {
            let rows = rows(&pool, s.project_id).await?;
            assert_eq!(
                row(&rows, "seat_mail", "SM-MAIL-COORDINATOR")?.repeat_count,
                2
            );
            assert_eq!(
                row(&rows, "seat_goal_feedback", "SM-FEEDBACK")?.repeat_count,
                2
            );
            assert_eq!(row(&rows, "peer_message", "SM-MAIL-PEER")?.repeat_count, 1);
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn seat_memory_labels_mail_with_sender_and_role() -> anyhow::Result<()> {
    with_scenario(
        "seat_memory_labels_mail_with_sender_and_role",
        |pool, s| async move {
            let rows = rows(&pool, s.project_id).await?;
            let mail = row(&rows, "seat_mail", "SM-MAIL-COORDINATOR")?;
            assert_eq!(mail.author, "coordinator");
            assert_eq!(
                mail.from_session.as_deref(),
                Some(s.coordinator.session_uuid.as_str())
            );
            assert_eq!(mail.from_role.as_deref(), Some("coordinator"));
            assert_eq!(mail.session_uuid, s.worker.session_uuid);
            let peer = row(&rows, "peer_message", "SM-MAIL-PEER")?;
            assert_eq!(peer.author, "peer");
            assert_eq!(
                peer.from_session.as_deref(),
                Some(s.peer.session_uuid.as_str())
            );
            assert_eq!(peer.from_role.as_deref(), Some("worker-b"));
            let brief = row(&rows, "seat_brief", "SM-BRIEF-WORKER")?;
            assert!(brief.from_session.is_none() && brief.from_role.is_none());
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn seat_memory_labels_reports_with_run_task_and_verdict() -> anyhow::Result<()> {
    with_scenario(
        "seat_memory_labels_reports_with_run_task_and_verdict",
        |pool, s| async move {
            let rows = rows(&pool, s.project_id).await?;
            let report = row(&rows, "seat_report", "SM-REPORT-NOTE")?;
            assert_eq!(report.run_id.as_deref(), Some(s.run.as_str()));
            assert_eq!(report.seat.as_deref(), Some("worker-a"));
            assert_eq!(report.task.as_deref(), Some(TASK_ACCEPTED));
            assert_eq!(report.author, "seat");
            assert!(report.report_filed);
            assert!(
                report.verdict_accepted,
                "latest verdict per criterion passed"
            );
            let coordinator = row(&rows, "seat_brief", "SM-BRIEF-COORDINATOR")?;
            assert!(
                !coordinator.report_filed,
                "the coordinator filed no report itself"
            );
            let peer = row(&rows, "seat_brief", "SM-BRIEF-PEER")?;
            assert_eq!(peer.task.as_deref(), Some(TASK_REJECTED));
            assert!(
                !peer.verdict_accepted,
                "one failed criterion rejects the task"
            );
            let template = row(&rows, "seat_brief", "SM-BRIEF-TEMPLATE")?;
            assert!(
                template.report_filed,
                "a template seat that reported is kept"
            );
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn seat_memory_survives_malformed_event_payloads() -> anyhow::Result<()> {
    with_scenario(
        "seat_memory_survives_malformed_event_payloads",
        |pool, s| async move {
            let extra = session(&pool, s.project_id).await?;
            let spec = LaunchSpec {
                run: Some(&s.run),
                seat: "worker-m",
                brief: "SM-BRIEF-MALFORMED",
                result: "success",
            };
            launch(&pool, &extra, &spec).await?;
            event(
                &pool,
                &s.run,
                Some(&extra.session_uuid),
                Some("T4"),
                "report_filed",
                &json!({"note_uuid": "not-a-uuid"}),
            )
            .await?;
            event(
                &pool,
                &s.run,
                None,
                Some("T4"),
                "verdict_recorded",
                &json!({"criterion": "C1", "passed": "yes"}),
            )
            .await?;
            event(
                &pool,
                &s.run,
                None,
                Some("T4"),
                "verdict_recorded",
                &json!({}),
            )
            .await?;
            let rows = rows(&pool, s.project_id).await?;
            assert_eq!(kinds(&rows, "seat_report").len(), 2);
            let brief = row(&rows, "seat_brief", "SM-BRIEF-MALFORMED")?;
            assert_eq!(brief.task.as_deref(), Some("T4"));
            assert!(brief.report_filed);
            assert!(!brief.verdict_accepted);
            Ok(())
        },
    )
    .await
}
