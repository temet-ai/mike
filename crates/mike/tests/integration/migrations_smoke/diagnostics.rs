use crate::test_db;

use super::fixtures::{insert_request_diagnostics, insert_request_with_diagnostics};
use serde_json::json;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn request_diagnostics_stores_all_findings() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping migrations smoke");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let long_match: String = "abcdefghij".repeat(40);
    let diagnostics = json!([
        {
            "category": "weasel",
            "analyzer": "weasel-pattern",
            "advice": "rephrase",
            "matched_text": "I'll be honest, this isn't great.",
            "match_range": {"start": 12, "end": 44},
            "other_hits": [
                {"start": 100, "end": 110}
            ],
            "created_at": "2026-05-02T10:00:00Z"
        },
        {
            "category": "structural-mismatch",
            "analyzer": "prompt-shape",
            "advice": "system block 2 missing",
            "matched_text": long_match,
            "other_hits": []
        },
        {
            "category": "weasel",
            "analyzer": "second-analyzer",
            "advice": "tone",
            "matched_text": "perhaps",
            "match_range": {"start": 0, "end": 7}
        }
    ]);

    let (req_id, req_created_at) = insert_request_with_diagnostics(&pool, &diagnostics).await?;
    insert_request_diagnostics(&pool, req_id, req_created_at, &diagnostics).await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.diagnostics
         WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 3, "expected 3 diagnostics rows after backfill");

    let rows = sqlx::query(
        "SELECT category, analyzer, advice, matched_text, match_range, other_hits, created_at
         FROM requests.diagnostics
         WHERE request_id = $1
         ORDER BY analyzer, category",
    )
    .bind(req_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(rows.len(), 3);

    let r0 = &rows[0];
    let r0_analyzer: String = r0.get("analyzer");
    let r0_category: String = r0.get("category");
    let r0_matched: String = r0.get("matched_text");
    let r0_advice: String = r0.get("advice");
    let r0_match_range: Option<serde_json::Value> = r0.get("match_range");
    let r0_other_hits: serde_json::Value = r0.get("other_hits");
    assert_eq!(r0_analyzer, "prompt-shape");
    assert_eq!(r0_category, "structural-mismatch");
    assert_eq!(r0_advice, "system block 2 missing");
    assert_eq!(
        r0_matched.len(),
        400,
        "matched_text must NOT be truncated by the column"
    );
    assert_eq!(r0_matched, long_match);
    assert!(r0_match_range.is_none());
    assert_eq!(r0_other_hits, json!([]));

    let r1 = &rows[1];
    let r1_analyzer: String = r1.get("analyzer");
    let r1_category: String = r1.get("category");
    let r1_matched: String = r1.get("matched_text");
    let r1_match_range: Option<serde_json::Value> = r1.get("match_range");
    let r1_other_hits: serde_json::Value = r1.get("other_hits");
    assert_eq!(r1_analyzer, "second-analyzer");
    assert_eq!(r1_category, "weasel");
    assert_eq!(r1_matched, "perhaps");
    assert_eq!(r1_match_range, Some(json!({"start": 0, "end": 7})));
    assert_eq!(r1_other_hits, json!([]));

    let r2 = &rows[2];
    let r2_analyzer: String = r2.get("analyzer");
    let r2_category: String = r2.get("category");
    let r2_matched: String = r2.get("matched_text");
    let r2_match_range: Option<serde_json::Value> = r2.get("match_range");
    let r2_other_hits: serde_json::Value = r2.get("other_hits");
    assert_eq!(r2_analyzer, "weasel-pattern");
    assert_eq!(r2_category, "weasel");
    assert_eq!(r2_matched, "I'll be honest, this isn't great.");
    assert_eq!(r2_match_range, Some(json!({"start": 12, "end": 44})));
    assert_eq!(r2_other_hits, json!([{"start": 100, "end": 110}]));

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn fk_cascade_deletes_request_diagnostics() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping migrations smoke");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let diagnostics = json!([
        {
            "category": "weasel",
            "analyzer": "weasel-pattern",
            "advice": "rephrase",
            "matched_text": "to be honest"
        }
    ]);
    let (req_id, req_created_at) = insert_request_with_diagnostics(&pool, &diagnostics).await?;
    insert_request_diagnostics(&pool, req_id, req_created_at, &diagnostics).await?;

    let pre: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.diagnostics
         WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(pre, 1, "expected 1 diagnostic row before parent delete");

    sqlx::query("DELETE FROM requests.requests WHERE id = $1 AND created_at = $2")
        .bind(req_id)
        .bind(req_created_at)
        .execute(&pool)
        .await?;

    let post: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests.diagnostics
         WHERE request_id = $1",
    )
    .bind(req_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        post, 0,
        "ON DELETE CASCADE must remove request_diagnostics rows when the parent is deleted"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
