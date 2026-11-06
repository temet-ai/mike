use crate::test_db;

use super::fixtures::{insert_bridge_row, insert_request, sha_placeholder};
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn messages_dedup_tables_apply_against_seeded_state() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping seeded-state test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (req_id, _req_created) = insert_request(&pool).await?;

    let sha = sha_placeholder();
    let body = serde_json::json!({"role": "user", "content": "hi"});
    for _ in 0..2 {
        sqlx::query!(
            "INSERT INTO messages.messages
                (message_sha256, body, byte_size, role_id)
             SELECT $1, $2, $3,
                 (SELECT id FROM messages.roles WHERE code = $4 LIMIT 1)
             WHERE NOT EXISTS (
                 SELECT 1 FROM messages.messages m WHERE m.message_sha256 = $1
             )",
            sha,
            body,
            20_i32,
            "user",
        )
        .execute(&pool)
        .await?;
    }
    let unique_messages: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages.messages WHERE message_sha256 = $1")
            .bind(&sha)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        unique_messages, 1,
        "two inserts of the same SHA must produce one messages row"
    );

    for ord in 0..2 {
        insert_bridge_row(&pool, req_id, ord, &sha).await?;
    }
    let bridge_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.messages
         WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(bridge_rows, 2, "two bridge rows for one request");

    let distinct_message_ids: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT message_id) FROM requests.messages
         WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        distinct_message_ids, 1,
        "both bridge rows must point at the same canonical messages.id"
    );

    let recovered_shas: Vec<String> = sqlx::query_scalar(
        "SELECT m.message_sha256
           FROM requests.messages rm
           JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = $1
          ORDER BY rm.ord ASC",
    )
    .bind(req_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        recovered_shas,
        vec![sha.clone(), sha.clone()],
        "both bridge rows must recover the seeded content sha via JOIN on message_id"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
