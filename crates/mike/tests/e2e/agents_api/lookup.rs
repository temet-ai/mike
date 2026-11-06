use crate::db_helpers;

use serde_json::Value;

use super::fixtures::seed_agent;

#[tokio::test]
async fn get_agent_by_uuid_returns_detail() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_shared_db("http://upstream-not-used-by-this-test.invalid")
            .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let agent_id = seed_agent(&pool, "researcher-test").await?;
    let resp = client
        .get(format!("{inspect_url}/api/agents/{agent_id}"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        200,
        "GET /api/agents/{{id}} must succeed for a seeded agent",
    );
    let body: Value = resp.json().await?;
    assert_eq!(body["id"].as_i64(), Some(agent_id));
    assert_eq!(body["name"].as_str(), Some("researcher-test"));
    assert!(
        body["compositions"].is_array(),
        "detail must include the (possibly empty) compositions array"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_agent_by_name_returns_detail_after_fix() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_shared_db("http://upstream-not-used-by-this-test.invalid")
            .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let agent_id = seed_agent(&pool, "frontend-developer").await?;

    let resp = client
        .get(format!("{inspect_url}/api/agents/frontend-developer"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        200,
        "GET /api/agents/frontend-developer must resolve via name fallback (was 400 before fix)",
    );
    let body: Value = resp.json().await?;
    assert_eq!(
        body["id"].as_i64(),
        Some(agent_id),
        "name lookup must return the same row as the direct id lookup"
    );
    assert_eq!(body["name"].as_str(), Some("frontend-developer"));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_agent_by_name_resolves_and_applies_patch() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_shared_db("http://upstream-not-used-by-this-test.invalid")
            .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let agent_id = seed_agent(&pool, "ui-designer").await?;

    let resp = client
        .patch(format!("{inspect_url}/api/agents/ui-designer"))
        .json(&serde_json::json!({
            "description": "design-system specialist"
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await?;
    assert_eq!(
        body["id"].as_i64(),
        Some(agent_id),
        "PATCH via name must mutate the same row as PATCH via bigint id"
    );
    assert_eq!(
        body["description"].as_str(),
        Some("design-system specialist")
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delete_agent_by_name_removes_row() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_shared_db("http://upstream-not-used-by-this-test.invalid")
            .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let agent_id = seed_agent(&pool, "watchdog").await?;
    let resp = client
        .delete(format!("{inspect_url}/api/agents/watchdog"))
        .send()
        .await?;
    assert_eq!(resp.status(), 204);

    let second = client
        .delete(format!("{inspect_url}/api/agents/watchdog"))
        .send()
        .await?;
    assert_eq!(second.status(), 404);

    let by_id = client
        .get(format!("{inspect_url}/api/agents/{agent_id}"))
        .send()
        .await?;
    assert_eq!(by_id.status(), 404);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_agent_unknown_name_returns_404_not_400() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_shared_db("http://upstream-not-used-by-this-test.invalid")
            .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{inspect_url}/api/agents/nonexistent-agent"))
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        404,
        "unknown name must return 404, not 400 — the segment is well-formed and the resource simply doesn't exist",
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn get_agent_oversized_segment_returns_400() -> anyhow::Result<()> {
    let Some(proxy) =
        db_helpers::start_proxy_with_shared_db("http://upstream-not-used-by-this-test.invalid")
            .await?
    else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let oversized = "a".repeat(300);
    let resp = client
        .get(format!("{inspect_url}/api/agents/{oversized}"))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
