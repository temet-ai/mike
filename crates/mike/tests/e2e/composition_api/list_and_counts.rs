use crate::db_helpers;
use crate::upstream;

use serde_json::Value;

use super::support::{create_subagent_via_api, create_via_api, prompt_sha256_for};

#[tokio::test]
async fn list_returns_seeded_compositions() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let baseline: Vec<Value> = client
        .get(format!("{inspect_url}/api/prompts/compositions"))
        .send()
        .await?
        .json()
        .await?;
    let baseline_count = baseline.len();

    let prompt_sha256 = prompt_sha256_for("seed-list-block2");
    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "seed-list",
        "block2-text",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id missing in envelope: {envelope}"))?
        .to_string();

    let listed: Vec<Value> = client
        .get(format!("{inspect_url}/api/prompts/compositions"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        listed.len(),
        baseline_count + 1,
        "creating one composition must add exactly one row to the baseline, got {listed:?}"
    );
    let created = listed
        .iter()
        .find(|r| r["id"].as_str() == Some(composition_id.as_str()))
        .ok_or_else(|| {
            anyhow::anyhow!("created composition {composition_id} not in list: {listed:?}")
        })?;
    assert_eq!(
        created["prompt_sha256"].as_str(),
        Some(prompt_sha256.as_str())
    );
    assert_eq!(created["label"].as_str(), Some("seed-list"));
    assert_eq!(created["block_count"].as_u64(), Some(3));
    assert_eq!(created["version"].as_i64(), Some(1));
    assert_eq!(created["applied_count_24h"].as_i64(), Some(0));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_compositions_returns_correct_applied_counts_for_many_rows() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let baseline_count = client
        .get(format!("{inspect_url}/api/prompts/compositions"))
        .send()
        .await?
        .json::<Vec<Value>>()
        .await?
        .len();

    let mut shas = Vec::with_capacity(12);
    for i in 0..12u32 {
        let sha = prompt_sha256_for(&format!("batch-list-block2-{i}"));
        create_subagent_via_api(
            &client,
            &inspect_url,
            &sha,
            &format!("batch-list-{i}"),
            &format!("body-{i}"),
            &format!("batch-list-agent-{i}"),
        )
        .await?;
        shas.push(sha);
    }

    let listed: Vec<Value> = client
        .get(format!("{inspect_url}/api/prompts/compositions"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        listed.len(),
        baseline_count + 12,
        "expected baseline + 12 compositions in list, got {} ({listed:?})",
        listed.len()
    );

    let mut response_counts = std::collections::HashMap::new();
    for row in &listed {
        let sha = row["prompt_sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("row missing prompt_sha256: {row}"))?;
        let count = row["applied_count_24h"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("row missing applied_count_24h: {row}"))?;
        response_counts.insert(sha.to_string(), count);
    }

    for (i, sha) in shas.iter().enumerate() {
        let count = response_counts
            .get(sha)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("sha {i} missing from list: {sha}"))?;
        assert_eq!(
            count, 0,
            "composition {sha} has no applied rows; batch handler must default to 0"
        );
    }

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
