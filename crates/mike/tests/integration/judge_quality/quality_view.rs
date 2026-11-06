use super::fixtures::{bootstrap_project_session, resolve_outcome_id};

use crate::test_db;

use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;
use uuid::Uuid;

#[tokio::test]
async fn insert_verdict_roundtrips_through_quality_view() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let project_path = format!("/test/judge-quality/{}", Uuid::new_v4().simple());
    let (_project_id, session_pk, session_uuid) =
        bootstrap_project_session(&pool, &project_path).await?;

    let outcome_id = resolve_outcome_id(&pool, "success").await?;
    let quality_score: f64 = 0.875;
    let rationale = "The session demonstrated clear goal achievement with minimal drift.";
    let judge_model = "test-judge/v0";
    let rubric_version: i32 = 1;

    let verdict_id: i64 = sqlx::query_scalar(
        "INSERT INTO session_judges.verdicts
             (session_id, outcome_id, quality_score, rationale, judge_model, rubric_version)
         VALUES ($1, $2, $3::numeric(4,3), $4, $5, $6)
         RETURNING id",
    )
    .bind(session_pk)
    .bind(outcome_id)
    .bind(quality_score)
    .bind(rationale)
    .bind(judge_model)
    .bind(rubric_version)
    .fetch_one(&pool)
    .await?;

    assert!(verdict_id > 0, "verdict id must be positive");

    let row: (String, Option<f64>, Option<String>, String, i32) = sqlx::query_as(
        "SELECT outcome,
                quality_score::float8,
                rationale,
                judge_model,
                rubric_version
           FROM mike.session_quality_v
          WHERE session_id = $1",
    )
    .bind(&session_uuid)
    .fetch_one(&pool)
    .await?;

    let (outcome, fetched_score, fetched_rationale, fetched_model, fetched_rubric) = row;
    assert_eq!(outcome, "success", "outcome code must match");
    let score = fetched_score.ok_or_else(|| anyhow::anyhow!("quality_score must not be null"))?;
    assert!(
        (score - 0.875).abs() < 1e-6,
        "quality_score roundtrip: expected 0.875, got {score}"
    );
    assert_eq!(
        fetched_rationale.as_deref(),
        Some(rationale),
        "rationale must round-trip verbatim"
    );
    assert_eq!(fetched_model, judge_model, "judge_model must match");
    assert_eq!(fetched_rubric, rubric_version, "rubric_version must match");

    handle.cleanup().await?;
    Ok(())
}
