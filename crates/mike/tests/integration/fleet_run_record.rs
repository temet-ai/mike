use mike::launch::fleet::run_record::summarise;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::fleet::{
    ForkMeasure, InterventionMeasure, NewFleet, OperationMeasure, QuestionMeasure, ReportMeasure,
    RunMeasures, RunPatterns, TimingMeasure,
};
use mike::store::persistence::fleet_repo::FleetPersist as _;
use serde_json::json;
use uuid::Uuid;

use crate::test_db::TestDb;

fn measures(time_to_done: f64) -> RunMeasures {
    RunMeasures {
        seats: Vec::new(),
        reports: ReportMeasure::default(),
        mail: Vec::new(),
        operations: OperationMeasure::default(),
        questions: QuestionMeasure::default(),
        forks: ForkMeasure::default(),
        timing: TimingMeasure {
            time_to_done_minutes: time_to_done,
            no_seat_minutes: 12.0,
            task_count: 2,
            ..Default::default()
        },
        tokens: Vec::new(),
        interventions: InterventionMeasure::default(),
    }
}

async fn fleet_with_run(handle: &PersistHandle) -> anyhow::Result<(Uuid, String)> {
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO launch.runs (run_id, backend) VALUES ($1, 'herdr')")
        .bind(&run_id)
        .execute(handle.pool())
        .await?;
    let fleet = handle
        .fleet()
        .insert_fleet(&NewFleet {
            slug: format!("record-{}", Uuid::new_v4()),
            project_path: "/tmp/fleet-record".to_owned(),
            definition: json!({}),
            missing: json!([]),
            created_by: None,
        })
        .await?;
    Ok((fleet.fleet_uuid, run_id))
}

#[tokio::test]
async fn a_recomputed_record_replaces_the_stored_one_and_keeps_the_four_numbers()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let (fleet, run_id) = fleet_with_run(&handle).await?;

    assert!(
        handle
            .fleet()
            .insert_run_record(fleet, &run_id, &summarise(&measures(100.0)))
            .await?
    );
    assert!(
        !handle
            .fleet()
            .insert_run_record(fleet, &run_id, &summarise(&measures(5.0)))
            .await?
    );
    assert!(
        handle
            .fleet()
            .replace_run_record(fleet, &run_id, &summarise(&measures(200.0)))
            .await?
    );

    let record = handle
        .fleet()
        .find_run_record(fleet)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no record was stored"))?;
    assert_eq!(record["measure_version"], "run-record-v3");
    assert_eq!(record["time_to_done_minutes"], 200.0);
    assert_eq!(record["fault_minutes"]["no_seat"], 12.0);
    assert_eq!(record["fault_minutes"]["total"], 12.0);
    assert_eq!(record["tokens_per_accepted_task"]["accepted_tasks"], 2);
    assert_eq!(record["manual_interventions"]["total"], 0);
    assert!(record.get("score").is_none());
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_run_with_no_launches_measures_zero_without_failing() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let run_id = format!("fleet-run-{}", Uuid::new_v4());
    let patterns = RunPatterns {
        operations: "a^",
        git_read: "a^",
        git_write: "a^",
    };

    let measured = handle.fleet().run_measures(&run_id, patterns).await?;

    assert_eq!(measured.timing, TimingMeasure::default());
    assert!(measured.tokens.is_empty());
    assert_eq!(measured.interventions, InterventionMeasure::default());
    db.cleanup().await?;
    Ok(())
}
