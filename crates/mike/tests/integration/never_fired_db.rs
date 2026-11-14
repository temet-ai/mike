use crate::test_db::{self, TestDb};

use mike::steerer::structural::never_fired::{
    NeverFiredRule, check_never_fired_context_kind, check_never_written_fleet_event_kind,
};

async fn seed_send_session_message(
    pool: &sqlx::PgPool,
    label: &str,
    kinds: &[&str],
) -> anyhow::Result<i64> {
    let project_id = test_db::seed_project(pool, label).await?;
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let other_uuid = uuid::Uuid::new_v4().to_string();
    test_db::seed_two_sessions(pool, &session_uuid, &other_uuid, project_id).await?;
    test_db::seed_session_turns(pool, &session_uuid, project_id, 1).await?;

    let session_db_id: i64 = sqlx::query_scalar!(
        "SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
        session_uuid,
    )
    .fetch_one(pool)
    .await?;
    let request_id: i64 = sqlx::query_scalar!(
        "SELECT id FROM requests.requests WHERE session_id = $1 LIMIT 1",
        session_db_id,
    )
    .fetch_one(pool)
    .await?;

    let context: Vec<serde_json::Value> = kinds
        .iter()
        .map(|k| serde_json::json!({"kind": k}))
        .collect();
    sqlx::query!(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, session_id, ord, tool_name, input)
         VALUES ($1, $2, $3, 0, 'send_session_message', $4)",
        format!("toolu_test_{session_db_id}"),
        request_id,
        session_db_id,
        serde_json::json!({"context": context}),
    )
    .execute(pool)
    .await?;

    Ok(session_db_id)
}

#[tokio::test]
async fn check_never_fired_context_kind_flags_a_declared_kind_that_never_fired()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    seed_send_session_message(&pool, "never-fired-gap-a", &["note"]).await?;
    seed_send_session_message(&pool, "never-fired-gap-b", &["file"]).await?;

    let rule = NeverFiredRule {
        name: "test rule",
        declared: &["note", "file", "url"],
    };
    let finding = check_never_fired_context_kind(&pool, &rule).await?;

    assert_eq!(finding.never_observed, vec!["url"]);
    assert_eq!(finding.total_observations, 2);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn check_never_fired_context_kind_reports_clean_when_every_declared_kind_fired()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    seed_send_session_message(&pool, "never-fired-clean-a", &["note", "file"]).await?;

    let rule = NeverFiredRule {
        name: "test rule",
        declared: &["note", "file"],
    };
    let finding = check_never_fired_context_kind(&pool, &rule).await?;

    assert!(finding.never_observed.is_empty());
    assert_eq!(finding.total_observations, 1);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn check_never_written_fleet_event_kind_counts_only_recent_rows_of_declared_kinds()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;

    sqlx::query("INSERT INTO launch.runs (run_id, backend) VALUES ('wiring-run', 'herdr')")
        .execute(&pool)
        .await?;
    for (kind, age_days) in [
        ("floor_passed", 0),
        ("commit_landed", 0),
        ("seat_woken", 10),
    ] {
        sqlx::query(
            "INSERT INTO launch.fleet_events (run_id, kind, occurred_at)
             VALUES ('wiring-run', $1, now() - make_interval(days => $2))",
        )
        .bind(kind)
        .bind(age_days)
        .execute(&pool)
        .await?;
    }

    let rule = NeverFiredRule {
        name: "test rule",
        declared: &["floor_passed", "seat_woken", "tree_broken_by"],
    };
    let finding = check_never_written_fleet_event_kind(&pool, &rule, 7).await?;

    assert_eq!(finding.never_observed, vec!["seat_woken", "tree_broken_by"]);
    assert_eq!(finding.total_observations, 2);

    db.cleanup().await?;
    Ok(())
}
