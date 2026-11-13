use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::test_support::seed_fleet_running;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::seed::{seed_launch, seed_request, seed_run, seed_session};
use super::{Fixture, fixture, new_fleet};
use crate::launch::fleet::run_record::write;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::test_support::seed_project;

struct Run {
    f: Fixture,
    row: FleetRow,
    run_id: String,
    worker: i64,
    acceptance: i64,
}

async fn seed_use(
    pool: &PgPool,
    (request_id, session_id, ord): (i64, i64, i32),
    tool: &str,
    command: Option<&str>,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let input = command.map(|c| json!({"command": c}));
    sqlx::query(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, session_id, ord, tool_name, input, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(format!("toolu_{}", Uuid::new_v4()))
    .bind(request_id)
    .bind(session_id)
    .bind(ord)
    .bind(tool)
    .bind(input)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_mail(
    pool: &PgPool,
    from: i64,
    to: i64,
    body: &str,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body, queued_at, consumed_at)
         VALUES ($1, $2, $3, $4, $5, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(to)
    .bind(from)
    .bind(body)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn closed_run() -> anyhow::Result<Option<Run>> {
    let Some(f) = fixture().await? else {
        return Ok(None);
    };
    let pool = f.handle.pool();
    let project_id = seed_project(pool, "/tmp/fleet-a").await?;
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&f.handle, &run_id).await?;
    let fleet = f
        .handle
        .fleet()
        .insert_fleet(&new_fleet("record", "/tmp/fleet-a"))
        .await?;
    seed_fleet_running(&f.handle, fleet.fleet_uuid, &run_id).await?;
    let worker_uuid = Uuid::new_v4();
    let acceptance_uuid = Uuid::new_v4();
    let worker = seed_session(pool, project_id, worker_uuid).await?;
    let acceptance = seed_session(pool, project_id, acceptance_uuid).await?;
    seed_launch(&f.handle, &run_id, "db-dev-T7", Some(worker_uuid), true).await?;
    seed_launch(
        &f.handle,
        &run_id,
        "acceptance",
        Some(acceptance_uuid),
        true,
    )
    .await?;
    let row = f
        .handle
        .fleet()
        .find_fleet(&fleet.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("the fleet row is gone"))?;
    Ok(Some(Run {
        f,
        row,
        run_id,
        worker,
        acceptance,
    }))
}

async fn seed_traffic(run: &Run) -> anyhow::Result<()> {
    let pool = run.f.handle.pool();
    let project_id = sqlx::query_scalar::<_, i64>("SELECT id FROM projects.projects LIMIT 1")
        .fetch_one(pool)
        .await?;
    let start = Utc::now() + Duration::seconds(5);
    let first = seed_request(
        pool,
        run.worker,
        project_id,
        None,
        start + Duration::minutes(1),
    )
    .await?;
    let second = seed_request(
        pool,
        run.worker,
        project_id,
        None,
        start + Duration::minutes(2),
    )
    .await?;
    seed_request(
        pool,
        run.worker,
        project_id,
        None,
        start + Duration::minutes(10),
    )
    .await?;
    let at = start + Duration::minutes(1);
    seed_use(pool, (first, run.worker, 0), "Bash", Some("git status"), at).await?;
    seed_use(
        pool,
        (first, run.worker, 1),
        "Bash",
        Some("scripts/cx check -p mike"),
        at,
    )
    .await?;
    seed_use(
        pool,
        (second, run.worker, 0),
        "mcp__mike__fleet_commit",
        None,
        at + Duration::minutes(2),
    )
    .await?;
    seed_mail(
        pool,
        run.worker,
        run.acceptance,
        "final report: the slice is done",
        start + Duration::minutes(3),
    )
    .await?;
    seed_report_note(pool, run.worker, start + Duration::minutes(3)).await?;
    seed_mail(
        pool,
        run.acceptance,
        run.worker,
        "pass",
        start + Duration::minutes(5),
    )
    .await?;
    Ok(())
}

async fn seed_report_note(pool: &PgPool, worker: i64, at: DateTime<Utc>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO workspace.session_notes (session_id, kind, title, body, created_at)
         VALUES ($1, 'report', 'slice done', 'the slice is done', $2)",
    )
    .bind(worker)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn the_close_writes_one_record_from_the_seeded_capture_rows() -> anyhow::Result<()> {
    let Some(mut run) = closed_run().await? else {
        return Ok(());
    };
    run.row.definition["execution"] = serde_json::json!({"operations": {"fixture": {
        "command": "scripts/cx check -p mike", "purpose": "fixture check"
    }}});
    seed_traffic(&run).await?;

    assert!(write(&run.f.handle, &run.row, &run.run_id).await?);
    let record = run
        .f
        .handle
        .fleet()
        .find_run_record(run.row.fleet_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no record was stored"))?;

    assert_eq!(record["run_id"], run.run_id.as_str());
    assert_eq!(record["measure_version"], "run-record-v3");
    assert_eq!(record["seat_count"], 2);
    assert_eq!(record["worker_count"], 1);
    assert_eq!(record["worker_tool_calls"], 3);
    assert_eq!(record["worker_git_reads"], 1);
    assert_eq!(record["operation_commands"], 1);
    assert_eq!(record["reports"], 1);
    assert_eq!(record["reports_answered"], 1);
    assert_eq!(record["report_to_verdict_median_minutes"], 2.0);
    assert_eq!(record["forks"], 0);
    let seats = record["seats"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("seats"))?;
    let worker = seats
        .iter()
        .find(|s| s["seat"] == "db-dev-T7")
        .ok_or_else(|| anyhow::anyhow!("no worker seat"))?;
    assert_eq!(worker["requests"], 3);
    assert_eq!(worker["working_minutes"], 1.0);
    assert_eq!(worker["waiting_minutes"], 8.0);
    assert_eq!(worker["calls_to_first_commit"], 3);
    assert_eq!(record["mail_by_pair"].as_array().map(Vec::len), Some(2));
    run.f.db.cleanup().await
}

#[tokio::test]
async fn closing_a_second_time_keeps_the_first_record() -> anyhow::Result<()> {
    let Some(run) = closed_run().await? else {
        return Ok(());
    };
    seed_traffic(&run).await?;

    assert!(write(&run.f.handle, &run.row, &run.run_id).await?);
    assert!(!write(&run.f.handle, &run.row, &run.run_id).await?);
    let stored = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM launch.run_records WHERE fleet_uuid = $1",
    )
    .bind(run.row.fleet_uuid)
    .fetch_one(run.f.handle.pool())
    .await?;
    assert_eq!(stored, 1);
    run.f.db.cleanup().await
}

#[tokio::test]
async fn a_run_nobody_launched_into_still_gets_an_empty_record() -> anyhow::Result<()> {
    let Some(run) = closed_run().await? else {
        return Ok(());
    };
    let empty = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&run.f.handle, &empty).await?;
    assert!(write(&run.f.handle, &run.row, &empty).await?);
    let record = run
        .f
        .handle
        .fleet()
        .find_run_record(run.row.fleet_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no record was stored"))?;
    assert_eq!(record["seat_count"], 0);
    assert_eq!(record["git_reads_per_100_calls"], serde_json::Value::Null);
    run.f.db.cleanup().await
}

#[tokio::test]
async fn a_run_that_does_not_exist_is_refused_and_a_fleet_without_a_record_reads_none()
-> anyhow::Result<()> {
    let Some(run) = closed_run().await? else {
        return Ok(());
    };
    assert!(
        run.f
            .handle
            .fleet()
            .find_run_record(run.row.fleet_uuid)
            .await?
            .is_none()
    );
    assert!(
        write(&run.f.handle, &run.row, "fleet-run-missing")
            .await
            .is_err()
    );
    assert!(
        run.f
            .handle
            .fleet()
            .find_run_record(Uuid::new_v4())
            .await?
            .is_none()
    );
    run.f.db.cleanup().await
}
