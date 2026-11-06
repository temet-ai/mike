use crate::seed;
use crate::test_db;

use super::fixtures::{SeedDeltaRowParams, seed_delta_row, seed_message, seed_request_row};
use chrono::{TimeZone, Utc};
use seed::{seed_project, seed_session};
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn request_delta_predecessor_fk_set_null_on_request_delete() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("test-project-predecessor-set-null-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let sid_pred_setnull = uuid::Uuid::new_v4().to_string();
    seed_session(&pool, project_id, &sid_pred_setnull).await?;
    let pred_created_at = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();
    let cur_created_at = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 1).unwrap();
    let predecessor_id =
        seed_request_row(&pool, project_id, &sid_pred_setnull, pred_created_at).await?;
    let request_id = seed_request_row(&pool, project_id, &sid_pred_setnull, cur_created_at).await?;

    let sha = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let body = serde_json::json!({"role": "user", "content": "delta msg"});
    let message_id = seed_message(&pool, &sha, "user", body.clone()).await?;
    seed_delta_row(
        &pool,
        SeedDeltaRowParams {
            request_id,
            predecessor_request_id: Some(predecessor_id),
            session_id: &sid_pred_setnull,
            change_kind: "added",
            ord: 0,
            message_id,
            message_sha256: &sha,
            removed_reason: None,
        },
    )
    .await?;

    let affected = sqlx::query("DELETE FROM requests.requests WHERE id = $1")
        .bind(predecessor_id)
        .execute(&pool)
        .await?
        .rows_affected();
    assert_eq!(affected, 1, "predecessor request must be deleted");

    let (surviving, pred_after): (i64, Option<i64>) = sqlx::query_as(
        "SELECT request_id, predecessor_request_id
           FROM requests.deltas
          WHERE request_id = $1",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        surviving, request_id,
        "the delta row must survive the predecessor delete"
    );
    assert_eq!(
        pred_after, None,
        "predecessor_request_id must be SET NULL after the predecessor request is deleted"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn lesson_request_id_fk_set_null_on_request_delete() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let project_id = seed_project(
        &pool,
        &format!("test-project-lesson-set-null-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let sid_lesson_setnull = uuid::Uuid::new_v4().to_string();
    seed_session(&pool, project_id, &sid_lesson_setnull).await?;
    let created_at = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();
    let request_id = seed_request_row(&pool, project_id, &sid_lesson_setnull, created_at).await?;

    let lesson_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, request_id, kind_id, text)
         SELECT $1, $2, k.id, 'set-null fixture lesson'
         FROM memory.lesson_kinds k
         WHERE k.code = 'convention'
         RETURNING id",
    )
    .bind(project_id)
    .bind(request_id)
    .fetch_one(&pool)
    .await?;

    let affected = sqlx::query("DELETE FROM requests.requests WHERE id = $1")
        .bind(request_id)
        .execute(&pool)
        .await?
        .rows_affected();
    assert_eq!(affected, 1, "request must be deleted");

    let (surviving, req_after): (i64, Option<i64>) =
        sqlx::query_as("SELECT id, request_id FROM memory.lessons WHERE id = $1")
            .bind(lesson_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        surviving, lesson_id,
        "the lesson row must survive the request delete"
    );
    assert_eq!(
        req_after, None,
        "lessons.request_id must be SET NULL after the referenced request is deleted"
    );

    test_db.cleanup().await?;
    Ok(())
}
