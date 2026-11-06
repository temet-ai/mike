use crate::db_helpers::TestDb;
use chrono::{TimeZone, Utc};
use sqlx::postgres::PgPoolOptions;

use super::{seed_block, seed_project, seed_request_row, seed_session, sha256_hex};

#[tokio::test]
async fn stream_block_rows_round_trip_ordered_by_seq_ord() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let project_id = seed_project(&pool, "test-blocks-roundtrip").await?;
    seed_session(&pool, project_id, "a1eebc99-9c0b-4ef8-bb6d-6bb9bd380a11").await?;
    let created_at = Utc.with_ymd_and_hms(2026, 5, 12, 10, 0, 0).unwrap();
    seed_request_row(
        &pool,
        project_id,
        8001,
        "a1eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        created_at,
    )
    .await?;

    seed_block(
        &pool,
        8001,
        "a1eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        2,
        "world!",
        "text",
    )
    .await?;
    seed_block(
        &pool,
        8001,
        "a1eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        0,
        "Hello ",
        "text",
    )
    .await?;
    seed_block(
        &pool,
        8001,
        "a1eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        1,
        "let me think",
        "thinking",
    )
    .await?;

    type Row = (i32, String, i32, String, String);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT b.seq_ord, b.content, b.byte_size, b.content_sha256, bt.code AS block_type
           FROM responses.stream_blocks b
           JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
          WHERE b.request_id = $1
          ORDER BY b.seq_ord ASC",
    )
    .bind(8001_i64)
    .fetch_all(&pool)
    .await?;

    assert_eq!(rows.len(), 3, "expected 3 block rows");
    let expected: &[(i32, &str, &str)] = &[
        (0, "Hello ", "text"),
        (1, "let me think", "thinking"),
        (2, "world!", "text"),
    ];
    for (i, (seq, content, btype)) in expected.iter().enumerate() {
        let (r_seq, r_content, r_byte, r_sha, r_btype) = &rows[i];
        assert_eq!(r_seq, seq);
        assert_eq!(r_content.as_str(), *content);
        assert_eq!(*r_byte, content.len() as i32, "byte_size must be len");
        assert_eq!(r_sha, &sha256_hex(content), "content_sha256 must match");
        assert_eq!(r_btype.as_str(), *btype);
    }

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn string_agg_reconstructs_text_blocks_only() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let project_id = seed_project(&pool, "test-blocks-stringagg").await?;
    seed_session(&pool, project_id, "a2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22").await?;
    let created_at = Utc.with_ymd_and_hms(2026, 5, 12, 10, 1, 0).unwrap();
    seed_request_row(
        &pool,
        project_id,
        8002,
        "a2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22",
        created_at,
    )
    .await?;

    seed_block(
        &pool,
        8002,
        "a2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22",
        2,
        "!",
        "text",
    )
    .await?;
    seed_block(
        &pool,
        8002,
        "a2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22",
        0,
        "Hello",
        "text",
    )
    .await?;
    seed_block(
        &pool,
        8002,
        "a2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22",
        3,
        "think-block",
        "thinking",
    )
    .await?;
    seed_block(
        &pool,
        8002,
        "a2eebc99-9c0b-4ef8-bb6d-6bb9bd380a22",
        1,
        " world",
        "text",
    )
    .await?;

    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT string_agg(b.content, '' ORDER BY b.seq_ord)
           FROM responses.stream_blocks b
           JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
          WHERE b.request_id = $1 AND bt.code = 'text'",
    )
    .bind(8002_i64)
    .fetch_optional(&pool)
    .await?;

    let reconstructed = row.and_then(|(s,)| s).unwrap_or_default();
    assert_eq!(
        reconstructed, "Hello world!",
        "string_agg must concatenate only text blocks in seq_ord order"
    );

    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stream_block_insert_is_idempotent_on_conflict() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let test_db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let project_id = seed_project(&pool, "test-blocks-idempotent").await?;
    seed_session(&pool, project_id, "a3eebc99-9c0b-4ef8-bb6d-6bb9bd380a33").await?;
    let created_at = Utc.with_ymd_and_hms(2026, 5, 12, 10, 3, 0).unwrap();
    seed_request_row(
        &pool,
        project_id,
        8004,
        "a3eebc99-9c0b-4ef8-bb6d-6bb9bd380a33",
        created_at,
    )
    .await?;

    seed_block(
        &pool,
        8004,
        "a3eebc99-9c0b-4ef8-bb6d-6bb9bd380a33",
        0,
        "block",
        "text",
    )
    .await?;
    seed_block(
        &pool,
        8004,
        "a3eebc99-9c0b-4ef8-bb6d-6bb9bd380a33",
        0,
        "block",
        "text",
    )
    .await?;

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM responses.stream_blocks WHERE request_id = $1")
            .bind(8004_i64)
            .fetch_one(&pool)
            .await?;
    assert_eq!(count.0, 1, "ON CONFLICT DO NOTHING must keep exactly 1 row");

    test_db.cleanup().await?;
    Ok(())
}
