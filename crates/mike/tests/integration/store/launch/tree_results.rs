use chrono::{Duration, Utc};
use mike::store::persistence::fleet_repo::FleetPersist as _;
use mike::store::persistence::launch_repo::LaunchPersist as _;
use sqlx::PgPool;
use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::launch::{NewLaunch, NewRun};

use crate::seed::{SeedRequest, seed_project, seed_session};
use crate::test_db::TestDb;

async fn seed_bash_use_without_result(
    pool: &PgPool,
    session: Uuid,
    request_id: i64,
) -> anyhow::Result<i64> {
    let project = seed_project(pool, "/tmp/tree-results").await?;
    seed_session(pool, project, &session.to_string()).await?;
    SeedRequest::new(request_id, project)
        .session_uuid(&session.to_string())
        .insert(pool)
        .await?;
    sqlx::query(
        "INSERT INTO responses.responses (request_id, response_status, stop_reason_id, completed_at)
         SELECT $1, 200, sr.id, NOW()
         FROM responses.stop_reasons sr
         WHERE sr.code = 'tool_use'",
    )
    .bind(request_id)
    .execute(pool)
    .await?;
    let use_id: i64 = sqlx::query_scalar(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, ord, tool_name, input, session_id)
         SELECT $1, $2, 0, 'Bash', $3, s.id
         FROM sessions.sessions s
         WHERE s.session_uuid = $4
         RETURNING id",
    )
    .bind(format!("toolu_{request_id}"))
    .bind(request_id)
    .bind(serde_json::json!({"command": "cargo test -p mike"}))
    .bind(session)
    .fetch_one(pool)
    .await?;
    Ok(use_id)
}

#[tokio::test]
async fn a_seats_command_still_running_is_listed_with_no_result() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session = Uuid::new_v4();
    let use_id = seed_bash_use_without_result(handle.pool(), session, 71_001).await?;
    handle
        .launch()
        .upsert_run(&NewRun {
            run_id: "fleet-tree-results-1".to_owned(),
            project_path: Some("/tmp/tree-results".to_owned()),
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let mut launch = NewLaunch::new("herdr", "/tmp/tree-results");
    launch.session_uuid = Some(session);
    launch.run_id = Some("fleet-tree-results-1".to_owned());
    launch.seat = Some("rust-developer-T1".to_owned());
    handle.launch().insert_launch(&launch).await?;

    let rows = handle
        .fleet()
        .tree_results(
            "fleet-tree-results-1",
            Some("cargo test"),
            Utc::now() - Duration::hours(1),
            10,
        )
        .await?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].use_id, use_id);
    assert_eq!(rows[0].seat.as_deref(), Some("rust-developer-T1"));
    assert_eq!(rows[0].command.as_deref(), Some("cargo test -p mike"));
    assert!(rows[0].started_at.is_some());
    assert!(rows[0].finished_at.is_none());
    assert!(rows[0].is_error.is_none());
    assert!(rows[0].content.is_none());
    let none = handle
        .fleet()
        .tree_results(
            "fleet-tree-results-1",
            Some("pnpm"),
            Utc::now() - Duration::hours(1),
            10,
        )
        .await?;
    assert!(none.is_empty());
    db.cleanup().await?;
    Ok(())
}
