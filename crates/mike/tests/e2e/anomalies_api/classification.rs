use crate::db_helpers;

use serde_json::json;

use super::fixtures::{
    fetch_anomalies, insert_request_row, mock_upstream_canned, prompt_sha256_for,
};

#[tokio::test]
async fn anomalies_empty_when_no_unknown_blocks_and_all_classes_known() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let known_sha = prompt_sha256_for("known-class-block2");
    let known_blocks = json!([
        {"type": "text", "text": "billing"},
        {"type": "text", "text": "preamble"},
        {"type": "text", "text": "known-class-block2"},
    ]);
    sqlx::query(
        "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks, first_seen_at, last_seen_at)
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(&known_sha)
    .bind(&known_blocks)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO overrides.overrides (prompt_sha256, system_prompt_id, label, blocks)
         VALUES ($1, (SELECT id FROM system_prompts.system_prompts WHERE prompt_sha256 = $1),
             'main-agent', $2::jsonb)",
    )
    .bind(&known_sha)
    .bind(&known_blocks)
    .execute(&pool)
    .await?;

    insert_request_row(
        &pool,
        "11111111-aaaa-bbbb-cccc-000000000001",
        1,
        Some(&known_blocks),
        Some(&known_sha),
        None,
    )
    .await?;

    let anomalies = fetch_anomalies(&client, &inspect_url).await?;
    assert!(
        anomalies.is_empty(),
        "expected no anomalies, got {anomalies:?}"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn anomalies_returns_structural_mismatch_group_for_unknown_blocks() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let session_id = "22222222-cccc-dddd-eeee-000000000002";
    let unknown_payload = json!({"reason": "non_3_block", "len": 4});
    insert_request_row(&pool, session_id, 42, None, None, Some(&unknown_payload)).await?;

    let anomalies = fetch_anomalies(&client, &inspect_url).await?;
    assert_eq!(
        anomalies.len(),
        1,
        "expected exactly one structural-mismatch group, got {anomalies:?}"
    );
    let group = &anomalies[0];
    assert_eq!(
        group["classification"].as_str(),
        Some("structural-mismatch")
    );
    assert!(
        group["prompt_sha256"].is_null(),
        "structural-mismatch group must carry prompt_sha256=null; got {group}"
    );
    assert!(
        group["block_text"].is_null(),
        "structural-mismatch group must carry block_text=null; got {group}"
    );
    assert_eq!(
        group["unknown_blocks"], unknown_payload,
        "unknown_blocks JSONB must round-trip verbatim"
    );
    assert_eq!(group["count"].as_i64(), Some(1));
    let observations = group["observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("observations must be array; got {group}"))?;
    assert_eq!(observations.len(), 1, "expected one observation");
    assert_eq!(observations[0]["request_id"].as_i64(), Some(42));
    assert_eq!(
        observations[0]["session_id"].as_str(),
        Some(session_id),
        "observation must pair request 42 with its own session"
    );

    let first_seen = group["first_seen"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("first_seen must be string"))?;
    let last_seen = group["last_seen"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("last_seen must be string"))?;
    assert!(chrono::DateTime::parse_from_rfc3339(first_seen).is_ok());
    assert!(chrono::DateTime::parse_from_rfc3339(last_seen).is_ok());

    let signature_hash = group["signature_hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("signature_hash must be string"))?;
    assert_eq!(
        signature_hash.len(),
        64,
        "signature_hash must be 64-char hex"
    );
    assert!(signature_hash.chars().all(|c| c.is_ascii_hexdigit()));

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn anomalies_returns_unknown_class_group_for_classified_request_without_composition()
-> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let session_id = "33333333-eeee-ffff-aaaa-000000000003";
    let novel_sha = prompt_sha256_for("novel-block2-text-not-yet-registered");
    let system_blocks = json!([
        {"type": "text", "text": "billing-header"},
        {"type": "text", "text": "identity preamble"},
        {"type": "text", "text": "novel-block2-text-not-yet-registered"},
    ]);
    insert_request_row(
        &pool,
        session_id,
        99,
        Some(&system_blocks),
        Some(&novel_sha),
        None,
    )
    .await?;

    let anomalies = fetch_anomalies(&client, &inspect_url).await?;
    assert_eq!(
        anomalies.len(),
        1,
        "expected exactly one unknown-class group, got {anomalies:?}"
    );
    let group = &anomalies[0];
    assert_eq!(group["classification"].as_str(), Some("unknown-class"));
    assert_eq!(group["prompt_sha256"].as_str(), Some(novel_sha.as_str()));
    assert_eq!(
        group["block_text"].as_str(),
        Some("novel-block2-text-not-yet-registered"),
        "block_text must carry the FULL system[2].text"
    );
    assert!(
        group["unknown_blocks"].is_null(),
        "unknown-class group must carry unknown_blocks=null; got {group}"
    );
    assert_eq!(
        group["signature_hash"].as_str(),
        Some(novel_sha.as_str()),
        "unknown-class signature_hash must equal prompt_sha256"
    );
    assert_eq!(group["count"].as_i64(), Some(1));
    let observations = group["observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("observations must be array"))?;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0]["request_id"].as_i64(), Some(99));
    assert_eq!(
        observations[0]["session_id"].as_str(),
        Some(session_id),
        "observation must pair request 99 with its own session"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn anomalies_excludes_classified_request_when_composition_exists() -> anyhow::Result<()> {
    let upstream = mock_upstream_canned().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let registered_sha = prompt_sha256_for("registered-class-block2");
    let registered_blocks = json!([
        {"type": "text", "text": "billing"},
        {"type": "text", "text": "preamble"},
        {"type": "text", "text": "registered-class-block2"},
    ]);
    sqlx::query(
        "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks, first_seen_at, last_seen_at)
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(&registered_sha)
    .bind(&registered_blocks)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO overrides.overrides (prompt_sha256, system_prompt_id, label, blocks)
         VALUES ($1, (SELECT id FROM system_prompts.system_prompts WHERE prompt_sha256 = $1),
             'main-agent', $2::jsonb)",
    )
    .bind(&registered_sha)
    .bind(&registered_blocks)
    .execute(&pool)
    .await?;

    insert_request_row(
        &pool,
        "44444444-1111-2222-3333-000000000004",
        7,
        Some(&registered_blocks),
        Some(&registered_sha),
        None,
    )
    .await?;

    let bad_sha = prompt_sha256_for("bad-class-block2");
    let bad_blocks = json!([
        {"type": "text", "text": "billing"},
        {"type": "text", "text": "preamble"},
        {"type": "text", "text": "bad-class-block2"},
    ]);
    insert_request_row(
        &pool,
        "44444444-1111-2222-3333-000000000005",
        8,
        Some(&bad_blocks),
        Some(&bad_sha),
        None,
    )
    .await?;

    let anomalies = fetch_anomalies(&client, &inspect_url).await?;
    assert_eq!(
        anomalies.len(),
        1,
        "expected exactly one anomaly group (the unregistered class); got {anomalies:?}"
    );
    let group = &anomalies[0];
    assert_eq!(group["prompt_sha256"].as_str(), Some(bad_sha.as_str()));
    let observations = group["observations"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("observations must be array"))?;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0]["request_id"].as_i64(), Some(8));
    assert_eq!(
        observations[0]["session_id"].as_str(),
        Some("44444444-1111-2222-3333-000000000005"),
        "observation must pair request 8 with its own session"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
