use super::fixtures::{
    assert_no_more, open_listener, recv_matching, recv_payload, seed_project_and_session,
    zero_vector_literal,
};

use crate::test_db;

use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn session_embeddings_session_id_is_the_row_key() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_notifications");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let (_project_id, session_pk, session_id) = seed_project_and_session(&pool).await?;
    let mut listener = open_listener(&handle.database_url).await?;
    let vector = zero_vector_literal();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO memory.session_embeddings
            (session_id, embedding, embedding_model, source, chunk_count, source_updated_at)
         VALUES ($1::text::uuid, $2::vector, 'test-model-v1', 'overview', 0, $3)",
    )
    .bind(&session_id)
    .bind(&vector)
    .bind(now)
    .execute(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["table"], "session_embeddings");
    assert_eq!(payload["op"], "INSERT");
    assert_eq!(payload["id"], session_id);
    assert_eq!(payload["session_id"], session_id);
    assert_no_more(&mut listener).await?;

    sqlx::query(
        "INSERT INTO memory.session_embeddings
            (session_id, embedding, embedding_model, source, chunk_count, source_updated_at)
         VALUES ($1::text::uuid, $2::vector, 'test-model-v2', 'overview', 1, $3)
         ON CONFLICT (session_id) DO UPDATE
            SET embedding_model = EXCLUDED.embedding_model,
                chunk_count     = EXCLUDED.chunk_count",
    )
    .bind(&session_id)
    .bind(&vector)
    .bind(now)
    .execute(&pool)
    .await?;
    let payload = recv_payload(&mut listener).await?;
    assert_eq!(payload["op"], "UPDATE");
    assert_eq!(payload["id"], session_id);
    assert_eq!(payload["session_id"], session_id);
    assert_no_more(&mut listener).await?;

    sqlx::query("DELETE FROM sessions.sessions WHERE id = $1")
        .bind(session_pk)
        .execute(&pool)
        .await?;
    let payload = recv_matching(&mut listener, "session_embeddings").await?;
    assert_eq!(payload["op"], "DELETE");
    assert_eq!(payload["id"], session_id);
    assert_eq!(
        payload["session_id"], session_id,
        "session_embeddings never degrades to global — session_id IS the row's own key"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
