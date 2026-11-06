use crate::db_helpers;

use serde_json::Value;

use super::fixtures::{
    canonical_tool_calls, insert_request_with_tool_calls, insert_subagent_tool_call,
};

#[tokio::test]
async fn list_session_tool_uses_scopes_to_one_agent() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_db("http://upstream-not-used-by-this-test.invalid").await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "a0000000-0000-4000-8000-00000000f134";
    let (tool_calls, ..) = canonical_tool_calls()?;
    insert_request_with_tool_calls(&pool, session_id, &tool_calls, 0).await?;
    insert_subagent_tool_call(
        &pool,
        session_id,
        "Rust Development Expert",
        "toolu_sub_a1",
        10,
    )
    .await?;
    insert_subagent_tool_call(
        &pool,
        session_id,
        "Rust Development Expert",
        "toolu_sub_a2",
        20,
    )
    .await?;
    insert_subagent_tool_call(
        &pool,
        session_id,
        "Verification Specialist",
        "toolu_sub_b1",
        30,
    )
    .await?;

    let client = reqwest::Client::new();
    let get = async |query: &str| -> anyhow::Result<Value> {
        let resp = client
            .get(format!(
                "{inspect_url}/api/sessions/{session_id}/tool_uses{query}"
            ))
            .send()
            .await?;
        assert_eq!(resp.status(), 200);
        Ok(resp.json().await?)
    };

    let all = get("").await?;
    assert_eq!(all["total_count"].as_i64(), Some(6));
    let primary_rows: Vec<&Value> = all["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?
        .iter()
        .filter(|r| r["subagent_persona"].is_null())
        .collect();
    assert_eq!(
        primary_rows.len(),
        3,
        "the parent's own three calls carry no persona"
    );

    for order in ["asc", "desc"] {
        let scoped = get(&format!("?agent=Rust%20Development%20Expert&order={order}")).await?;
        assert_eq!(
            scoped["total_count"].as_i64(),
            Some(2),
            "total_count must describe the filtered set, not the session"
        );
        let rows = scoped["rows"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?;
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert!(row["agent_name"].is_null());
            assert_eq!(
                row["subagent_persona"].as_str(),
                Some("Rust Development Expert")
            );
        }
    }

    let other = get("?agent=Verification%20Specialist").await?;
    assert_eq!(other["total_count"].as_i64(), Some(1));

    let none = get("?agent=no-such-agent").await?;
    assert_eq!(none["total_count"].as_i64(), Some(0));
    assert_eq!(
        none["rows"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("rows must be an array"))?
            .len(),
        0
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
