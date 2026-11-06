use crate::db_helpers;

use serde_json::{Value, json};

use super::fixtures::{canonical_tool_calls, insert_request_with_tool_calls};

#[tokio::test]
async fn list_session_tool_uses_returns_full_input_per_row() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "a0000000-0000-4000-8000-000000000001";
    let (tool_calls, edit_input, bash_input, write_input) = canonical_tool_calls()?;
    let request_id = insert_request_with_tool_calls(&pool, session_id, &tool_calls, 0).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/sessions/{session_id}/tool_uses"))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;

    assert_eq!(body["session_id"].as_str(), Some(session_id));
    assert_eq!(body["total_count"].as_i64(), Some(3));
    assert_eq!(body["offset"].as_u64(), Some(0));
    assert_eq!(body["limit"].as_u64(), Some(200));

    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be a JSON array"))?;
    assert_eq!(rows.len(), 3);

    assert_eq!(rows[0]["request_id"].as_i64(), Some(request_id));
    assert_eq!(rows[0]["session_id"].as_str(), Some(session_id));
    assert_eq!(rows[0]["tool_id"].as_str(), Some("toolu_write_003"));
    assert_eq!(rows[0]["tool_name"].as_str(), Some("Write"));
    assert_eq!(rows[0]["file_path"].as_str(), Some("/b"));
    assert_eq!(rows[0]["ord"].as_i64(), Some(3));
    assert_eq!(
        rows[0]["input"], write_input,
        "Write input must round-trip as the full JSON object"
    );
    assert!(
        rows[0]["created_at"].as_str().is_some(),
        "created_at is rendered as RFC 3339 string"
    );

    assert_eq!(rows[1]["tool_id"].as_str(), Some("toolu_bash_002"));
    assert_eq!(rows[1]["tool_name"].as_str(), Some("Bash"));
    assert!(
        rows[1]["file_path"].is_null(),
        "Bash has no file_path in input → file_path is null"
    );
    assert_eq!(rows[1]["ord"].as_i64(), Some(2));
    assert_eq!(rows[1]["input"], bash_input);

    assert_eq!(rows[2]["tool_id"].as_str(), Some("toolu_edit_001"));
    assert_eq!(rows[2]["tool_name"].as_str(), Some("Edit"));
    assert_eq!(rows[2]["file_path"].as_str(), Some("/a"));
    assert_eq!(rows[2]["ord"].as_i64(), Some(1));
    assert_eq!(
        rows[2]["input"], edit_input,
        "Edit input must round-trip as the full JSON object"
    );

    let asc_resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/tool_uses?order=asc"
        ))
        .send()
        .await?;
    assert_eq!(asc_resp.status(), 200);
    let asc_body: Value = asc_resp.json().await?;
    let asc_rows = asc_body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("asc rows must be a JSON array"))?;
    let asc_ords: Vec<i64> = asc_rows.iter().filter_map(|r| r["ord"].as_i64()).collect();
    assert_eq!(
        asc_ords,
        vec![1, 2, 3],
        "order=asc must return chronological (created_at ASC, ord ASC) order"
    );

    let bad = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/tool_uses?order=upside-down"
        ))
        .send()
        .await?;
    assert_eq!(bad.status(), 400);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_session_tool_uses_paginates_with_offset_and_limit() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "a0000000-0000-4000-8000-000000000002";
    let (tool_calls, _, _, _) = canonical_tool_calls()?;
    insert_request_with_tool_calls(&pool, session_id, &tool_calls, 0).await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{inspect_url}/api/sessions/{session_id}/tool_uses?offset=1&limit=1"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;

    assert_eq!(
        body["total_count"].as_i64(),
        Some(3),
        "total_count is the full set, NOT the page size"
    );
    assert_eq!(body["offset"].as_u64(), Some(1));
    assert_eq!(body["limit"].as_u64(), Some(1));

    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert_eq!(rows.len(), 1, "page size honours ?limit=1");
    assert_eq!(rows[0]["tool_name"].as_str(), Some("Bash"));
    assert_eq!(rows[0]["ord"].as_i64(), Some(2));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_session_tool_uses_unknown_session_returns_empty_envelope() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let client = reqwest::Client::new();
    let unknown = "ffffffff-ffff-4fff-8fff-ffffffffffff";
    let resp = client
        .get(format!("{inspect_url}/api/sessions/{unknown}/tool_uses"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        200,
        "unknown session returns 200 with empty envelope, not 404"
    );
    let body: Value = resp.json().await?;
    assert_eq!(body["session_id"].as_str(), Some(unknown));
    assert_eq!(body["total_count"].as_i64(), Some(0));
    assert_eq!(body["offset"].as_u64(), Some(0));
    assert_eq!(body["limit"].as_u64(), Some(200));
    let rows = body["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert!(rows.is_empty(), "unknown session has no rows");

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_session_tool_uses_isolates_sessions() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_a = "a0000000-0000-4000-8000-00000000000a";
    let session_b = "b0000000-0000-4000-8000-00000000000b";
    let (calls_a, _, _, _) = canonical_tool_calls()?;
    insert_request_with_tool_calls(&pool, session_a, &calls_a, 0).await?;

    let only_b = json!({"file_path": "/only-b", "content": "B content"});
    let calls_b = json!([
        {"tool_id": "toolu_b_only", "name": "Write", "input_json": serde_json::to_string(&only_b)?, "complete": true},
    ]);
    insert_request_with_tool_calls(&pool, session_b, &calls_b, 100).await?;

    let client = reqwest::Client::new();
    let resp_b = client
        .get(format!("{inspect_url}/api/sessions/{session_b}/tool_uses"))
        .send()
        .await?;
    assert_eq!(resp_b.status(), 200);
    let body_b: Value = resp_b.json().await?;
    assert_eq!(body_b["total_count"].as_i64(), Some(1));
    let rows_b = body_b["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    assert_eq!(rows_b.len(), 1);
    assert_eq!(rows_b[0]["session_id"].as_str(), Some(session_b));
    assert_eq!(rows_b[0]["tool_id"].as_str(), Some("toolu_b_only"));
    assert_eq!(rows_b[0]["input"], only_b);

    let resp_a = client
        .get(format!("{inspect_url}/api/sessions/{session_a}/tool_uses"))
        .send()
        .await?;
    let body_a: Value = resp_a.json().await?;
    assert_eq!(body_a["total_count"].as_i64(), Some(3));
    let rows_a = body_a["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
    for row in rows_a {
        assert_eq!(
            row["session_id"].as_str(),
            Some(session_a),
            "session A's page must not leak rows from session B"
        );
    }

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
