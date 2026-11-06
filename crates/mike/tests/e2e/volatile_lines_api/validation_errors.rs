use crate::db_helpers;
use serde_json::{Value, json};

use super::support::mock_upstream_canned;

#[tokio::test]
async fn invalid_regex_returns_400() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "bad-regex",
            "pattern": "(unclosed",
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("regex"),
        "400 body must mention regex; got: {err}"
    );
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn duplicate_name_returns_409() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let first = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "duplicate-name",
            "pattern": "^x=",
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(first.status(), 201);
    let second = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "duplicate-name",
            "pattern": "^y=",
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(second.status(), 409);
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_unknown_id_returns_404() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let unknown = uuid::Uuid::new_v4().hyphenated().to_string();
    let resp = client
        .patch(format!("{inspect_url}/api/volatile-lines/{unknown}"))
        .json(&json!({"enabled": false}))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delete_unknown_id_returns_404() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let unknown = uuid::Uuid::new_v4().hyphenated().to_string();
    let resp = client
        .delete(format!("{inspect_url}/api/volatile-lines/{unknown}"))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn patch_with_invalid_regex_returns_400() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let post = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "patch-bad-regex-seed",
            "pattern": "^valid=",
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(post.status(), 201);
    let created: Value = post.json().await?;
    let pattern_id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id missing on POST: {created}"))?
        .to_string();

    let resp = client
        .patch(format!("{inspect_url}/api/volatile-lines/{pattern_id}"))
        .json(&json!({"pattern": "(unclosed"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("regex"),
        "400 body must mention regex; got: {err}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_block_index_returns_400() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "bad-block-index",
            "pattern": "^x=",
            "applies_to_block_index": 5,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("0, 1, or 2") || err.contains("block_index"),
        "400 body must mention valid block index; got: {err}"
    );
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn pattern_too_long_returns_400() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let too_long = "a".repeat(501);
    let resp = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "too-long",
            "pattern": too_long,
            "applies_to_block_index": 2,
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains("length") && err.contains("500"),
        "400 body must mention the 500-char cap; got: {err}"
    );
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
