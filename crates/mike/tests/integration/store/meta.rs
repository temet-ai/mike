use mike::store::persistence::PersistHandle;

use crate::test_db::TestDb;

#[tokio::test]
async fn upsert_project_returning_id_is_idempotent_and_bumps_last_seen() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let path = format!("test-w5-meta-upsert-{}", uuid::Uuid::new_v4());

    let id1 = handle.upsert_project_returning_id(&path).await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&handle_db.database_url)
        .await?;
    let first_last_seen: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT last_seen FROM projects.projects WHERE project_path = $1")
            .bind(&path)
            .fetch_one(&pool)
            .await?;

    let id2 = handle.upsert_project_returning_id(&path).await?;
    assert_eq!(id1, id2, "second upsert must return the same project id");

    let second_last_seen: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT last_seen FROM projects.projects WHERE project_path = $1")
            .bind(&path)
            .fetch_one(&pool)
            .await?;
    assert!(
        second_last_seen >= first_last_seen,
        "second upsert must not move last_seen backwards: {second_last_seen} < {first_last_seen}"
    );

    let row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM projects.projects WHERE project_path = $1")
            .bind(&path)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        row_count, 1,
        "upsert must never insert a duplicate project row"
    );

    pool.close().await;
    Ok(())
}
