use crate::test_db;

use super::fixtures::{insert_bridge_row, insert_request, sha_placeholder};
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn messages_role_check_rejects_unknown_role() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping role-check test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let sha = sha_placeholder();
    let body = serde_json::json!({});
    let result = sqlx::query!(
        "INSERT INTO messages.messages
            (message_sha256, body, byte_size, role_id)
         VALUES ($1, $2, $3,
             (SELECT id FROM messages.roles WHERE code = 'tool' LIMIT 1))",
        sha,
        body,
        0_i32,
    )
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "unknown role 'tool' must be rejected (no roles row → NULL role_id)"
    );

    for role in ["user", "assistant", "system"] {
        let sha = sha_placeholder();
        sqlx::query!(
            "INSERT INTO messages.messages
                (message_sha256, body, byte_size, role_id)
             VALUES ($1, $2, $3,
                 (SELECT id FROM messages.roles WHERE code = $4 LIMIT 1))",
            sha,
            body,
            0_i32,
            role,
        )
        .execute(&pool)
        .await?;
    }

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn request_messages_ord_check_rejects_negative() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping ord-check test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (req_id, _req_created) = insert_request(&pool).await?;
    let sha = sha_placeholder();
    let message_id: i64 = sqlx::query_scalar(
        "INSERT INTO messages.messages
            (message_sha256, body, byte_size, role_id)
         VALUES ($1, '{}'::jsonb, 0,
             (SELECT id FROM messages.roles WHERE code = 'user' LIMIT 1))
         RETURNING id",
    )
    .bind(&sha)
    .fetch_one(&pool)
    .await?;

    let _ = &sha;
    let result = sqlx::query!(
        "INSERT INTO requests.messages
            (request_id, ord, message_id)
         VALUES ($1, $2, $3)",
        req_id,
        -1_i32,
        message_id,
    )
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "request_messages_ord_nonneg must reject negative ord"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn request_messages_fk_rejects_unknown_message_id() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping fk-message-id test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (req_id, _req_created) = insert_request(&pool).await?;
    let unknown_message_id: i64 = 9_999_999_999;

    let result = sqlx::query!(
        "INSERT INTO requests.messages
            (request_id, ord, message_id)
         VALUES ($1, 0, $2)",
        req_id,
        unknown_message_id,
    )
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "FK to messages(id) must reject a bridge row whose message_id has no canonical row"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn request_messages_fk_cascades_on_request_delete() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping cascade test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (req_id, _req_created) = insert_request(&pool).await?;
    let sha = sha_placeholder();
    sqlx::query!(
        "INSERT INTO messages.messages
            (message_sha256, body, byte_size, role_id)
         VALUES ($1, '{}'::jsonb, 0,
             (SELECT id FROM messages.roles WHERE code = 'user' LIMIT 1))",
        sha,
    )
    .execute(&pool)
    .await?;
    insert_bridge_row(&pool, req_id, 0, &sha).await?;
    insert_bridge_row(&pool, req_id, 1, &sha).await?;

    let pre: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.messages
         WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(pre, 2, "two bridge rows pre-delete");

    sqlx::query!("DELETE FROM requests.requests WHERE id = $1", req_id)
        .execute(&pool)
        .await?;

    let post: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.messages
         WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        post, 0,
        "ON DELETE CASCADE must remove request_messages rows on parent delete"
    );

    let messages_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages.messages WHERE message_sha256 = $1")
            .bind(&sha)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        messages_left, 1,
        "the canonical messages row must survive the request-level cascade; \
         orphan sweep is a future phase"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
