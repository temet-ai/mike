use crate::db_helpers;
use serde_json::{Value, json};

use super::support::mock_upstream_canned;

#[tokio::test]
async fn crud_roundtrip_for_volatile_pattern() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let empty: Vec<Value> = client
        .get(format!("{inspect_url}/api/volatile-lines"))
        .send()
        .await?
        .json()
        .await?;
    assert!(empty.is_empty(), "expected empty list, got {empty:?}");

    let post_resp = client
        .post(format!("{inspect_url}/api/volatile-lines"))
        .json(&json!({
            "name": "cc-version",
            "pattern": "^cc_version=",
            "applies_to_block_index": 2,
            "notes": "strip the per-process billing header"
        }))
        .send()
        .await?;
    assert_eq!(post_resp.status(), 201);
    let created: Value = post_resp.json().await?;
    let pattern_id = created["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("id missing on POST: {created}"))?
        .to_string();
    assert_eq!(created["name"].as_str(), Some("cc-version"));
    assert_eq!(created["pattern"].as_str(), Some("^cc_version="));
    assert_eq!(created["applies_to_block_index"].as_u64(), Some(2));
    assert_eq!(
        created["enabled"].as_bool(),
        Some(true),
        "enabled defaults to true server-side"
    );
    assert_eq!(
        created["notes"].as_str(),
        Some("strip the per-process billing header")
    );

    let by_id: Value = client
        .get(format!("{inspect_url}/api/volatile-lines/{pattern_id}"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        by_id["id"].as_i64().map(|n| n.to_string()).as_deref(),
        Some(pattern_id.as_str()),
        "GET by id must return the same numeric id"
    );

    let patch_resp = client
        .patch(format!("{inspect_url}/api/volatile-lines/{pattern_id}"))
        .json(&json!({"enabled": false}))
        .send()
        .await?;
    assert_eq!(patch_resp.status(), 200);
    let patched: Value = patch_resp.json().await?;
    assert_eq!(patched["enabled"].as_bool(), Some(false));

    let del_resp = client
        .delete(format!("{inspect_url}/api/volatile-lines/{pattern_id}"))
        .send()
        .await?;
    assert_eq!(del_resp.status(), 204);

    let after_del = client
        .get(format!("{inspect_url}/api/volatile-lines/{pattern_id}"))
        .send()
        .await?;
    assert_eq!(after_del.status(), 404);

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
