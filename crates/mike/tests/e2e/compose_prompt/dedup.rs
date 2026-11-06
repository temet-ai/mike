use crate::db_helpers;
use crate::upstream;

use super::support::{
    latest_request_id_for_session, prompt_sha256, send_canonical_request, wait_for_request_detail,
};

#[tokio::test]
async fn system_prompt_blob_dedupes_across_requests() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();
    let block2 = "dedupe-test-block-2-text";

    for i in 0..5u32 {
        let session_id = format!("11111111-1111-4111-8111-{i:012x}");
        let resp = send_canonical_request(&client, &proxy_url, &session_id, block2).await?;
        assert_eq!(resp.status(), 200);
        let request_id = latest_request_id_for_session(&client, &inspect_url, &session_id).await?;
        wait_for_request_detail(&client, &inspect_url, request_id).await?;
    }

    let (block2_rows,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT FROM system_prompts.system_prompts WHERE prompt_sha256 = $1",
    )
    .bind(prompt_sha256(block2))
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        block2_rows, 1,
        "system_prompts must dedupe by prompt_sha256: the 5 byte-identical block2 \
         requests must produce exactly ONE canonical row; got {block2_rows}"
    );

    let expected_sha = prompt_sha256(block2);
    let (sha_in_db,): (String,) = sqlx::query_as(
        "SELECT prompt_sha256 FROM system_prompts.system_prompts WHERE prompt_sha256 = $1",
    )
    .bind(&expected_sha)
    .fetch_one(&pool)
    .await?;
    assert_eq!(sha_in_db, expected_sha);

    let (rsp_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*)::BIGINT FROM requests.system_prompts")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        rsp_count, 5,
        "request_system_prompts must have 1 row per request; got {rsp_count}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn requests_table_no_longer_carries_system_prompt_fields() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let dropped: Vec<(String,)> = sqlx::query_as(
        "SELECT column_name::text
         FROM information_schema.columns
         WHERE table_schema = 'requests'
           AND table_name = 'requests'
           AND column_name IN (
               'system_prompt',
               'system_blocks',
               'system_class_id',
               'composition_applied',
               'unknown_blocks'
           )",
    )
    .fetch_all(&pool)
    .await?;
    assert!(
        dropped.is_empty(),
        "rev-3 migration must remove all 5 system-prompt columns; still present: {dropped:?}"
    );

    let tables: Vec<(String, String)> = sqlx::query_as(
        "SELECT table_schema::text, table_name::text
         FROM information_schema.tables
         WHERE (table_schema = 'system_prompts' AND table_name = 'system_prompts')
            OR (table_schema = 'requests'       AND table_name = 'system_prompts')
            OR (table_schema = 'overrides'      AND table_name = 'overrides')
         ORDER BY table_schema, table_name",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        tables.len(),
        3,
        "post-squash schema must have 3 key tables \
         (system_prompts.system_prompts, requests.system_prompts, overrides.overrides); \
         found: {tables:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
