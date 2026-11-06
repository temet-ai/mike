use crate::test_db;

use super::seed::{SEEDED_TABLES, seed_source};
use super::support::{BASELINE_TABLES, baseline_cursors, connect, count};
use mike::resync::ResyncEngine;
use sqlx::Row;
use test_db::TestDb;

#[tokio::test]
async fn replay_pending_copies_session_then_is_idempotent() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_engine integration test");
        return Ok(());
    };

    let source_handle = test_db.acquire_isolated().await?;
    let dest_handle = test_db.acquire_isolated().await?;
    let source = connect(&source_handle.database_url).await?;
    let dest = connect(&dest_handle.database_url).await?;

    baseline_cursors(&source, BASELINE_TABLES).await?;

    let seeded = seed_source(&source).await?;

    let engine = ResyncEngine::new(source.clone(), dest.clone());
    let report = engine.replay_pending().await?;
    eprintln!("first replay report: {report:?}");
    assert!(report.total_rows > 0, "first replay must insert rows");

    for (table, expected) in SEEDED_TABLES {
        let got = count(&dest, table).await?;
        assert_eq!(got, *expected, "dest count mismatch for {table}");
    }

    let joined_uuid: String = sqlx::query(
        "SELECT s.session_uuid::text AS u
           FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          ORDER BY r.id ASC LIMIT 1",
    )
    .fetch_one(&dest)
    .await?
    .get("u");
    assert_eq!(
        joined_uuid, seeded.session_uuid,
        "request.session_id must remap to dest session"
    );

    let resolved_wire: String = sqlx::query(
        "SELECT u.wire_tool_use_id AS w
           FROM tools.results res
           JOIN tools.uses u ON u.id = res.tool_use_id
          LIMIT 1",
    )
    .fetch_one(&dest)
    .await?
    .get("w");
    assert_eq!(
        resolved_wire, seeded.wire_tool_use_id,
        "tool_result must resolve its tool_use"
    );

    let stamped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM requests.requests r
           JOIN memory.operator_turns ot ON ot.id = r.operator_turn_id
          WHERE ot.session_id = r.session_id",
    )
    .fetch_one(&dest)
    .await?;
    assert_eq!(
        stamped, 2,
        "both requests' operator_turn_id must be remapped (cycle break)"
    );

    let turn_bounds_ok: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.operator_turns ot
           JOIN requests.requests rs ON rs.id = ot.start_request_id
           JOIN requests.requests re ON re.id = ot.end_request_id",
    )
    .fetch_one(&dest)
    .await?;
    assert_eq!(
        turn_bounds_ok, 1,
        "operator_turn start/end request FKs must remap"
    );

    let regenerated: Option<i64> =
        sqlx::query_scalar("SELECT input_tokens FROM responses.responses LIMIT 1")
            .fetch_one(&dest)
            .await?;
    assert_eq!(
        regenerated,
        Some(10),
        "dest must regenerate input_tokens from token_usage"
    );

    let report2 = engine.replay_pending().await?;
    eprintln!("second replay report: {report2:?}");
    assert_eq!(
        report2.total_rows, 0,
        "second replay must insert 0 rows (idempotent)"
    );

    for (table, expected) in SEEDED_TABLES {
        let got = count(&dest, table).await?;
        assert_eq!(
            got, *expected,
            "dest count changed on second replay for {table}"
        );
    }

    source.close().await;
    dest.close().await;
    source_handle.cleanup().await?;
    dest_handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn replay_pending_idempotent_when_stream_block_exists_on_dest() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset — skipping resync_engine stream-block test");
        return Ok(());
    };

    let source_handle = test_db.acquire_isolated().await?;
    let dest_handle = test_db.acquire_isolated().await?;
    let source = connect(&source_handle.database_url).await?;
    let dest = connect(&dest_handle.database_url).await?;

    baseline_cursors(&source, BASELINE_TABLES).await?;
    seed_source(&source).await?;

    let engine = ResyncEngine::new(source.clone(), dest.clone());
    let report = engine.replay_pending().await?;
    eprintln!("stream-block first replay report: {report:?}");
    assert_eq!(
        count(&dest, "responses.stream_blocks").await?,
        1,
        "first replay must copy the stream block to dest"
    );

    sqlx::query("DELETE FROM sync.cursor WHERE table_name = 'responses.stream_blocks'")
        .execute(&source)
        .await?;

    let report2 = engine.replay_pending().await?;
    eprintln!("stream-block re-sync report: {report2:?}");

    assert_eq!(
        count(&dest, "responses.stream_blocks").await?,
        1,
        "re-syncing an existing (request_id, seq_ord) must not duplicate the block"
    );

    source.close().await;
    dest.close().await;
    source_handle.cleanup().await?;
    dest_handle.cleanup().await?;
    Ok(())
}
