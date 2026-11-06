use super::fixtures::start_upstream_stub;

use crate::db_helpers;

#[tokio::test]
async fn backfill_maps_jsonb_array_to_ordered_rows() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping request_tools_roundtrip");
        return Ok(());
    };
    let (_proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let mut conn = pool.acquire().await?;
    sqlx::query(
        "CREATE TEMP TABLE legacy_tool_names (
             id BIGINT PRIMARY KEY,
             tool_names JSONB NOT NULL
         )",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO legacy_tool_names (id, tool_names)
         VALUES (1, $1), (2, '[]'::jsonb)",
    )
    .bind(serde_json::json!(["Bash", "Read", "Bash"]))
    .execute(&mut *conn)
    .await?;

    let rows: Vec<(i64, i32, String)> = sqlx::query_as(
        "SELECT r.id, (ord - 1)::int AS zero_based_ord, value
           FROM legacy_tool_names r
           CROSS JOIN LATERAL
               jsonb_array_elements_text(r.tool_names) WITH ORDINALITY AS t(value, ord)
          WHERE value IS NOT NULL
          ORDER BY r.id, zero_based_ord",
    )
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    assert_eq!(
        rows,
        vec![
            (1, 0, "Bash".to_string()),
            (1, 1, "Read".to_string()),
            (1, 2, "Bash".to_string()),
        ],
        "WITH ORDINALITY is 1-based; (ord - 1) must yield 0-based ord, \
         declaration order + duplicates preserved; the empty array (id 2) \
         contributes zero rows"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
